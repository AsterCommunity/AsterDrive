use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, IntoActiveModel};
use std::{fmt, sync::Arc};
use tokio::sync::Mutex;

use crate::db::repository::storage_policy_credential_repo;
use crate::entities::{storage_policy, storage_policy_credential};
use crate::errors::{AsterError, Result};
use crate::storage::drivers::onedrive::MicrosoftGraphAccessTokenProvider;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::{
    MicrosoftGraphCloud, StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus,
};

use super::audit::{
    OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED, OAUTH_AUDIT_EVENT_REAUTH_REQUIRED,
    OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_RECOVERED, OAUTH_AUDIT_RESULT_SUCCESS,
    StorageCredentialOauthAuditDetails, write_storage_credential_oauth_audit,
};
use super::microsoft::{
    MicrosoftTokenResponse, decrypt_stored_client_secret, metadata_string, parse_metadata,
    refresh_microsoft_graph_token,
};
use super::{crypto, normalize_optional_string, normalize_scopes, scopes_to_json};

#[derive(Debug)]
pub(crate) struct MicrosoftGraphCredentialTokenProvider {
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy_id: i64,
    cloud: MicrosoftGraphCloud,
    tenant: String,
    client_id: String,
    client_secret: Option<String>,
    cache: Mutex<MicrosoftGraphCredentialTokenCache>,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
}

#[derive(Debug)]
struct MicrosoftGraphCredentialTokenCache {
    access_token: String,
    expires_at: Option<chrono::DateTime<Utc>>,
    refresh_token_ciphertext: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct MicrosoftGraphTokenRefreshRequest {
    pub(super) cloud: MicrosoftGraphCloud,
    pub(super) tenant: String,
    pub(super) client_id: String,
    pub(super) client_secret: Option<String>,
    pub(super) refresh_token: String,
}

#[async_trait::async_trait]
pub(super) trait MicrosoftGraphTokenRefresher: Send + Sync + fmt::Debug {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse>;
}

#[derive(Debug)]
struct DefaultMicrosoftGraphTokenRefresher;

#[async_trait::async_trait]
impl MicrosoftGraphTokenRefresher for DefaultMicrosoftGraphTokenRefresher {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse> {
        refresh_microsoft_graph_token(
            request.cloud,
            &request.tenant,
            &request.client_id,
            request.client_secret.as_deref(),
            &request.refresh_token,
        )
        .await
    }
}

pub(crate) fn build_microsoft_graph_credential_token_provider(
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy: &storage_policy::Model,
    credential: &storage_policy_credential::Model,
    cloud: MicrosoftGraphCloud,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    build_microsoft_graph_credential_token_provider_with_refresher(
        db,
        encryption_key,
        policy,
        credential,
        cloud,
        Arc::new(DefaultMicrosoftGraphTokenRefresher),
    )
}

pub(super) fn build_microsoft_graph_credential_token_provider_with_refresher(
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy: &storage_policy::Model,
    credential: &storage_policy_credential::Model,
    cloud: MicrosoftGraphCloud,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    let metadata = parse_metadata(&credential.metadata);
    let access_token_ciphertext =
        credential
            .access_token_ciphertext
            .as_deref()
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    "storage credential is missing access token",
                )
            })?;
    let access_token = crypto::decrypt_token(
        &encryption_key,
        crypto::token_aad(
            credential.policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        access_token_ciphertext,
    )?;
    let client_id = normalize_optional_string(Some(policy.access_key.clone()))
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|metadata| metadata_string(metadata, "client_id"))
        })
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "storage credential is missing Microsoft Graph client_id; save the OneDrive policy application settings and reauthorize",
            )
        })?;
    let client_secret = match normalize_optional_string(Some(policy.secret_key.clone())) {
        Some(client_secret) => Some(client_secret),
        None => metadata
            .as_ref()
            .and_then(|metadata| metadata_string(metadata, "client_secret_ciphertext"))
            .map(|ciphertext| {
                decrypt_stored_client_secret(&encryption_key, credential.policy_id, &ciphertext)
            })
            .transpose()?,
    };
    Ok(Arc::new(MicrosoftGraphCredentialTokenProvider {
        db,
        encryption_key,
        policy_id: credential.policy_id,
        cloud,
        tenant: credential
            .tenant_id
            .clone()
            .filter(|tenant| !tenant.trim().is_empty())
            .unwrap_or_else(|| "common".to_string()),
        client_id,
        client_secret,
        cache: Mutex::new(MicrosoftGraphCredentialTokenCache {
            access_token,
            expires_at: credential.expires_at,
            refresh_token_ciphertext: credential.refresh_token_ciphertext.clone(),
        }),
        token_refresher,
    }))
}

#[async_trait::async_trait]
impl MicrosoftGraphAccessTokenProvider for MicrosoftGraphCredentialTokenProvider {
    async fn access_token(&self) -> Result<String> {
        {
            let cache = self.cache.lock().await;
            if cached_access_token_is_fresh(cache.expires_at) {
                return Ok(cache.access_token.clone());
            }
        }
        self.refresh_access_token().await
    }

    async fn refresh_access_token(&self) -> Result<String> {
        let mut cache = self.cache.lock().await;
        let Some(refresh_token_ciphertext) = cache.refresh_token_ciphertext.as_deref() else {
            self.mark_reauth_required("storage credential is missing refresh token")
                .await?;
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "storage credential is missing refresh token; reauthorize Microsoft Graph",
            ));
        };
        let used_refresh_token_ciphertext = refresh_token_ciphertext.to_string();
        let refresh_token = crypto::decrypt_token(
            &self.encryption_key,
            crypto::token_aad(
                self.policy_id,
                StorageCredentialProvider::MicrosoftGraph.as_str(),
                "refresh",
            )
            .as_bytes(),
            refresh_token_ciphertext,
        )?;
        let token = match self
            .token_refresher
            .refresh_token(MicrosoftGraphTokenRefreshRequest {
                cloud: self.cloud,
                tenant: self.tenant.clone(),
                client_id: self.client_id.clone(),
                client_secret: self.client_secret.clone(),
                refresh_token,
            })
            .await
        {
            Ok(token) => token,
            Err(error) => {
                if let Some(access_token) = self
                    .recover_from_rotated_refresh_token(&mut cache, &used_refresh_token_ciphertext)
                    .await?
                {
                    write_storage_credential_oauth_audit(
                        &self.db,
                        0,
                        StorageCredentialOauthAuditDetails {
                            event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                            result: OAUTH_AUDIT_RESULT_RECOVERED,
                            policy_id: Some(self.policy_id),
                            cloud: Some(self.cloud),
                            tenant: Some(&self.tenant),
                            reason: Some(
                                "refresh token was already rotated by another provider instance",
                            ),
                            recovered_from_token_rotation: Some(true),
                            ..Default::default()
                        },
                    )
                    .await;
                    return Ok(access_token);
                }
                let _ = self.mark_reauth_required(error.message()).await;
                return Err(storage_driver_error(
                    StorageErrorKind::Auth,
                    format!("refresh Microsoft Graph access token: {error}"),
                ));
            }
        };
        let now = Utc::now();
        let expires_at = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| now + Duration::seconds(seconds)));
        let access_aad = crypto::token_aad(
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        );
        let refresh_aad = crypto::token_aad(
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "refresh",
        );
        let access_token_ciphertext = crypto::encrypt_token(
            &self.encryption_key,
            access_aad.as_bytes(),
            &token.access_token,
        )?;
        let refresh_token_ciphertext = match token.refresh_token.as_deref() {
            Some(refresh_token) if !refresh_token.trim().is_empty() => Some(crypto::encrypt_token(
                &self.encryption_key,
                refresh_aad.as_bytes(),
                refresh_token,
            )?),
            _ => cache.refresh_token_ciphertext.clone(),
        };
        let scopes = if let Some(scope) = token.scope.as_deref() {
            let scopes = normalize_scopes(Some(
                scope.split_whitespace().map(ToOwned::to_owned).collect(),
            ));
            Some(scopes_to_json(&scopes)?)
        } else {
            None
        };
        let updated =
            storage_policy_credential_repo::update_oauth_refresh_result_if_refresh_token_matches(
                &self.db,
                storage_policy_credential_repo::OAuthRefreshUpdate {
                    policy_id: self.policy_id,
                    provider: StorageCredentialProvider::MicrosoftGraph,
                    credential_kind: StorageCredentialKind::OauthDelegated,
                    expected_refresh_token_ciphertext: &used_refresh_token_ciphertext,
                    access_token_ciphertext,
                    refresh_token_ciphertext: refresh_token_ciphertext.clone(),
                    expires_at,
                    scopes,
                    now,
                },
            )
            .await?;
        if !updated {
            if let Some(access_token) = self
                .recover_from_rotated_refresh_token(&mut cache, &used_refresh_token_ciphertext)
                .await?
            {
                write_storage_credential_oauth_audit(
                    &self.db,
                    0,
                    StorageCredentialOauthAuditDetails {
                        event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                        result: OAUTH_AUDIT_RESULT_RECOVERED,
                        policy_id: Some(self.policy_id),
                        cloud: Some(self.cloud),
                        tenant: Some(&self.tenant),
                        reason: Some(
                            "refresh token was already rotated by another provider instance",
                        ),
                        recovered_from_token_rotation: Some(true),
                        ..Default::default()
                    },
                )
                .await;
                return Ok(access_token);
            }
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Microsoft Graph refresh token was updated concurrently; retry the request with the latest credential state",
            ));
        }

        cache.access_token = token.access_token;
        cache.expires_at = expires_at;
        cache.refresh_token_ciphertext = refresh_token_ciphertext;
        write_storage_credential_oauth_audit(
            &self.db,
            0,
            StorageCredentialOauthAuditDetails {
                event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                result: OAUTH_AUDIT_RESULT_SUCCESS,
                policy_id: Some(self.policy_id),
                cloud: Some(self.cloud),
                tenant: Some(&self.tenant),
                refresh_token_rotated: Some(token.refresh_token.is_some()),
                ..Default::default()
            },
        )
        .await;
        Ok(cache.access_token.clone())
    }
}

impl MicrosoftGraphCredentialTokenProvider {
    async fn recover_from_rotated_refresh_token(
        &self,
        cache: &mut MicrosoftGraphCredentialTokenCache,
        used_refresh_token_ciphertext: &str,
    ) -> Result<Option<String>> {
        let Some(credential) = storage_policy_credential_repo::find_by_policy_provider_kind(
            &self.db,
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await?
        else {
            return Ok(None);
        };
        let Some(current_refresh_token_ciphertext) = credential.refresh_token_ciphertext.clone()
        else {
            return Ok(None);
        };
        if current_refresh_token_ciphertext == used_refresh_token_ciphertext {
            return Ok(None);
        }
        let Some(access_token_ciphertext) = credential.access_token_ciphertext.as_deref() else {
            return Ok(None);
        };
        let access_token = crypto::decrypt_token(
            &self.encryption_key,
            crypto::token_aad(
                self.policy_id,
                StorageCredentialProvider::MicrosoftGraph.as_str(),
                "access",
            )
            .as_bytes(),
            access_token_ciphertext,
        )?;
        if access_token.trim().is_empty() {
            return Ok(None);
        }

        cache.access_token = access_token;
        cache.expires_at = credential.expires_at;
        cache.refresh_token_ciphertext = Some(current_refresh_token_ciphertext);
        if cached_access_token_is_fresh(cache.expires_at) {
            return Ok(Some(cache.access_token.clone()));
        }

        Err(storage_driver_error(
            StorageErrorKind::Auth,
            "Microsoft Graph refresh token was already rotated; retry the request with the latest credential state",
        ))
    }

    async fn mark_reauth_required(&self, reason: &str) -> Result<()> {
        let Some(credential) = storage_policy_credential_repo::find_by_policy_provider_kind(
            &self.db,
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await?
        else {
            return Ok(());
        };
        let now = Utc::now();
        let mut active = credential.into_active_model();
        active.status = Set(StorageCredentialStatus::ReauthRequired);
        active.status_reason = Set(Some(reason.to_string()));
        active.updated_at = Set(now);
        active.update(&self.db).await.map_err(AsterError::from)?;
        write_storage_credential_oauth_audit(
            &self.db,
            0,
            StorageCredentialOauthAuditDetails {
                event: OAUTH_AUDIT_EVENT_REAUTH_REQUIRED,
                result: OAUTH_AUDIT_RESULT_FAILED,
                policy_id: Some(self.policy_id),
                cloud: Some(self.cloud),
                tenant: Some(&self.tenant),
                reason: Some(reason),
                ..Default::default()
            },
        )
        .await;
        Ok(())
    }
}

fn cached_access_token_is_fresh(expires_at: Option<chrono::DateTime<Utc>>) -> bool {
    expires_at.is_none_or(|expires_at| expires_at > Utc::now() + Duration::seconds(60))
}
