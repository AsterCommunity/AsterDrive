use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use std::{fmt, sync::Arc};
use tokio::sync::Mutex;

use crate::db::repository::storage_policy_connector_credential_repo;
use crate::errors::{AsterError, Result, storage_driver_error};
use crate::storage::drivers::onedrive::MicrosoftGraphAccessTokenProvider;
use aster_drive_model::entities::{storage_policy, storage_policy_connector_credential};
use aster_drive_model::types::{
    MicrosoftGraphCloud, StorageCredentialProvider, StorageCredentialStatus,
};
use aster_drive_storage::StorageErrorKind;

use super::oauth::{
    MicrosoftTokenResponse, decrypt_application_client_secret, refresh_microsoft_graph_token,
};
use super::{normalize_microsoft_graph_scopes, normalized_option};
use crate::services::storage_policy::credential::crypto;
use crate::services::storage_policy::credential::{
    OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED, OAUTH_AUDIT_EVENT_REAUTH_REQUIRED,
    OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_RECOVERED, OAUTH_AUDIT_RESULT_SUCCESS,
    StorageCredentialOauthAuditDetails, write_storage_credential_oauth_audit,
};

const REDACTED_SECRET: &str = "***REDACTED***";

pub(crate) struct MicrosoftGraphCredentialTokenProvider {
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy_id: i64,
    cache: Mutex<MicrosoftGraphConnectorCredentialCache>,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
}

pub(crate) struct MicrosoftGraphCleanupTokenProvider {
    encryption_key: String,
    policy_id: i64,
    cloud: MicrosoftGraphCloud,
    tenant: String,
    client_id: String,
    client_secret: Option<SecretString>,
    cache: Mutex<MicrosoftGraphCredentialTokenCache>,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
}

#[derive(Clone, Debug)]
pub(crate) struct MicrosoftGraphCleanupTokenSnapshot {
    pub(crate) cloud: MicrosoftGraphCloud,
    pub(crate) tenant_id: Option<String>,
    pub(crate) client_id: Option<String>,
    pub(crate) client_secret_ciphertext: Option<String>,
    pub(crate) access_token_ciphertext: String,
    pub(crate) refresh_token_ciphertext: Option<String>,
    pub(crate) expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug)]
struct MicrosoftGraphCredentialTokenCache {
    access_token: String,
    expires_at: Option<chrono::DateTime<Utc>>,
    refresh_token_ciphertext: Option<String>,
}

#[derive(Debug)]
struct MicrosoftGraphConnectorCredentialCache {
    credential: super::OneDriveCredentialV1,
    revision: i64,
}

#[derive(Clone)]
pub(super) struct MicrosoftGraphTokenRefreshRequest {
    pub(super) cloud: MicrosoftGraphCloud,
    pub(super) tenant: String,
    pub(super) client_id: String,
    pub(super) client_secret: Option<SecretString>,
    pub(super) refresh_token: SecretString,
}

impl fmt::Debug for MicrosoftGraphCredentialTokenProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicrosoftGraphCredentialTokenProvider")
            .field("policy_id", &self.policy_id)
            .field("cache", &REDACTED_SECRET)
            .field("token_refresher", &self.token_refresher)
            .finish()
    }
}

impl fmt::Debug for MicrosoftGraphCleanupTokenProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicrosoftGraphCleanupTokenProvider")
            .field("policy_id", &self.policy_id)
            .field("cloud", &self.cloud)
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| REDACTED_SECRET),
            )
            .field("cache", &REDACTED_SECRET)
            .field("token_refresher", &self.token_refresher)
            .finish()
    }
}

impl fmt::Debug for MicrosoftGraphTokenRefreshRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicrosoftGraphTokenRefreshRequest")
            .field("cloud", &self.cloud)
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| REDACTED_SECRET),
            )
            .field("refresh_token", &REDACTED_SECRET)
            .finish()
    }
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
            request.client_secret.as_ref(),
            request.refresh_token.expose_secret(),
        )
        .await
    }
}

pub(crate) fn build_microsoft_graph_credential_token_provider(
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy: &storage_policy::Model,
    credential: &storage_policy_connector_credential::Model,
    payload: super::OneDriveCredentialV1,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    build_microsoft_graph_credential_token_provider_with_refresher(
        db,
        encryption_key,
        policy,
        credential,
        payload,
        Arc::new(DefaultMicrosoftGraphTokenRefresher),
    )
}

pub(crate) fn build_microsoft_graph_cleanup_token_provider(
    encryption_key: String,
    policy: &storage_policy::Model,
    snapshot: MicrosoftGraphCleanupTokenSnapshot,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    build_microsoft_graph_cleanup_token_provider_with_refresher(
        encryption_key,
        policy,
        snapshot,
        Arc::new(DefaultMicrosoftGraphTokenRefresher),
    )
}

pub(super) fn build_microsoft_graph_cleanup_token_provider_with_refresher(
    encryption_key: String,
    policy: &storage_policy::Model,
    snapshot: MicrosoftGraphCleanupTokenSnapshot,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    let access_token = decrypt_oauth_token(
        &encryption_key,
        policy.id,
        "access",
        &snapshot.access_token_ciphertext,
    )?;
    let client_id = snapshot
        .client_id
        .and_then(|value| normalized_option(Some(value)))
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "storage cleanup credential is missing Microsoft Graph client_id snapshot",
            )
        })?;
    let client_secret = snapshot
        .client_secret_ciphertext
        .and_then(|value| normalized_option(Some(value)))
        .map(|ciphertext| {
            decrypt_application_client_secret(&encryption_key, policy.id, &ciphertext)
        })
        .transpose()?
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "storage cleanup credential is missing Microsoft Graph client_secret snapshot",
            )
        })?;
    Ok(Arc::new(MicrosoftGraphCleanupTokenProvider {
        encryption_key,
        policy_id: policy.id,
        cloud: snapshot.cloud,
        tenant: snapshot
            .tenant_id
            .and_then(|tenant| normalized_option(Some(tenant)))
            .unwrap_or_else(|| "common".to_string()),
        client_id,
        client_secret: Some(client_secret),
        cache: Mutex::new(MicrosoftGraphCredentialTokenCache {
            access_token,
            expires_at: snapshot.expires_at,
            refresh_token_ciphertext: snapshot.refresh_token_ciphertext,
        }),
        token_refresher,
    }))
}

fn decrypt_oauth_token(
    encryption_key: &str,
    policy_id: i64,
    token_name: &str,
    ciphertext: &str,
) -> Result<String> {
    crypto::decrypt_token(
        encryption_key,
        crypto::token_aad(
            policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            token_name,
        )
        .as_bytes(),
        ciphertext,
    )
}

pub(super) fn build_microsoft_graph_credential_token_provider_with_refresher(
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy: &storage_policy::Model,
    credential: &storage_policy_connector_credential::Model,
    payload: super::OneDriveCredentialV1,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    debug_assert_eq!(
        policy.id, credential.policy_id,
        "Microsoft Graph credential must belong to the supplied storage policy"
    );
    let application = &payload.application;
    if normalized_option(Some(application.client_id.clone())).is_none() {
        return Err(storage_driver_error(
            StorageErrorKind::Auth,
            "OneDrive connector credential is missing Microsoft Graph client_id",
        ));
    }
    if normalized_option(Some(application.client_secret.clone())).is_none() {
        return Err(storage_driver_error(
            StorageErrorKind::Auth,
            "OneDrive connector credential is missing Microsoft Graph client_secret",
        ));
    }
    let authorization = payload.authorization.as_ref().ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Auth,
            "OneDrive connector credential has not been authorized",
        )
    })?;
    if authorization.status != StorageCredentialStatus::Authorized {
        return Err(storage_driver_error(
            StorageErrorKind::Auth,
            "OneDrive connector credential requires authorization",
        ));
    }
    if authorization.access_token.trim().is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Auth,
            "OneDrive connector credential is missing access token",
        ));
    }
    Ok(Arc::new(MicrosoftGraphCredentialTokenProvider {
        db,
        encryption_key,
        policy_id: credential.policy_id,
        cache: Mutex::new(MicrosoftGraphConnectorCredentialCache {
            credential: payload,
            revision: credential.revision,
        }),
        token_refresher,
    }))
}

#[async_trait::async_trait]
impl MicrosoftGraphAccessTokenProvider for MicrosoftGraphCredentialTokenProvider {
    async fn access_token(&self) -> Result<String> {
        {
            let cache = self.cache.lock().await;
            let authorization = cache.credential.authorization.as_ref().ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    "OneDrive connector credential has not been authorized",
                )
            })?;
            if cached_access_token_is_fresh(authorization.expires_at) {
                return Ok(authorization.access_token.clone());
            }
        }
        self.refresh_access_token().await
    }

    async fn refresh_access_token(&self) -> Result<String> {
        let mut cache = self.cache.lock().await;
        let application = cache.credential.application.clone();
        let authorization = cache.credential.authorization.as_ref().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "OneDrive connector credential has not been authorized",
            )
        })?;
        let Some(refresh_token) = authorization.refresh_token.clone() else {
            self.mark_reauth_required_locked(
                &mut cache,
                "storage credential is missing refresh token",
            )
            .await?;
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "storage credential is missing refresh token; reauthorize Microsoft Graph",
            ));
        };
        let used_revision = cache.revision;
        let token = match self
            .token_refresher
            .refresh_token(MicrosoftGraphTokenRefreshRequest {
                cloud: application.cloud,
                tenant: application.tenant.clone(),
                client_id: application.client_id.clone(),
                client_secret: Some(SecretString::from(application.client_secret.clone())),
                refresh_token: SecretString::from(refresh_token),
            })
            .await
        {
            Ok(token) => token,
            Err(error) => {
                if let Some(access_token) = self
                    .recover_from_concurrent_refresh(&mut cache, used_revision)
                    .await?
                {
                    let fields = microsoft_graph_audit_fields(
                        application.cloud,
                        &application.tenant,
                        None,
                        Some(true),
                    );
                    write_storage_credential_oauth_audit(
                        &self.db,
                        0,
                        StorageCredentialOauthAuditDetails {
                            event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                            result: OAUTH_AUDIT_RESULT_RECOVERED,
                            policy_id: Some(self.policy_id),
                            connector_id: Some(super::OneDriveConnector::ID),
                            provider: Some(StorageCredentialProvider::MicrosoftGraph),
                            reason: Some(
                                "refresh token was already rotated by another provider instance",
                            ),
                            fields: Some(&fields),
                        },
                    )
                    .await;
                    return Ok(access_token);
                }
                let kind = error.storage_error_kind().unwrap_or(StorageErrorKind::Auth);
                if matches!(kind, StorageErrorKind::Auth | StorageErrorKind::Permission) {
                    let _ = self
                        .mark_reauth_required_locked(&mut cache, error.message())
                        .await;
                }
                return Err(storage_driver_error(
                    kind,
                    format!("refresh Microsoft Graph access token: {error}"),
                ));
            }
        };
        let now = Utc::now();
        let expires_at = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| now + Duration::seconds(seconds)));
        let refreshed_scopes = token.scope.as_deref().map(|scope| {
            normalize_microsoft_graph_scopes(
                Some(scope.split_whitespace().map(ToOwned::to_owned).collect()),
                "",
            )
        });
        let refresh_token_rotated = token
            .refresh_token
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let authorization = cache.credential.authorization.as_mut().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "OneDrive connector credential has not been authorized",
            )
        })?;
        authorization.access_token = token.access_token;
        authorization.expires_at = expires_at;
        authorization.last_refreshed_at = Some(now);
        authorization.status = StorageCredentialStatus::Authorized;
        authorization.status_reason = None;
        if let Some(scopes) = refreshed_scopes {
            authorization.scopes = scopes;
        }
        if let Some(refresh_token) = token.refresh_token.filter(|value| !value.trim().is_empty()) {
            authorization.refresh_token = Some(refresh_token);
        }
        let access_token = authorization.access_token.clone();
        let updated = self
            .persist_cache_if_revision(&cache, used_revision)
            .await?;
        if !updated {
            if let Some(access_token) = self
                .recover_from_concurrent_refresh(&mut cache, used_revision)
                .await?
            {
                let fields = microsoft_graph_audit_fields(
                    application.cloud,
                    &application.tenant,
                    None,
                    Some(true),
                );
                write_storage_credential_oauth_audit(
                    &self.db,
                    0,
                    StorageCredentialOauthAuditDetails {
                        event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                        result: OAUTH_AUDIT_RESULT_RECOVERED,
                        policy_id: Some(self.policy_id),
                        connector_id: Some(super::OneDriveConnector::ID),
                        provider: Some(StorageCredentialProvider::MicrosoftGraph),
                        reason: Some(
                            "refresh token was already rotated by another provider instance",
                        ),
                        fields: Some(&fields),
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
        cache.revision = cache.revision.checked_add(1).ok_or_else(|| {
            AsterError::database_operation("storage connector credential revision overflow")
        })?;
        let fields = microsoft_graph_audit_fields(
            application.cloud,
            &application.tenant,
            Some(refresh_token_rotated),
            None,
        );
        write_storage_credential_oauth_audit(
            &self.db,
            0,
            StorageCredentialOauthAuditDetails {
                event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                result: OAUTH_AUDIT_RESULT_SUCCESS,
                policy_id: Some(self.policy_id),
                connector_id: Some(super::OneDriveConnector::ID),
                provider: Some(StorageCredentialProvider::MicrosoftGraph),
                fields: Some(&fields),
                ..Default::default()
            },
        )
        .await;
        Ok(access_token)
    }
}

#[async_trait::async_trait]
impl MicrosoftGraphAccessTokenProvider for MicrosoftGraphCleanupTokenProvider {
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
        // Cleanup tasks run from a deleted-policy snapshot. Do not write audit
        // records or mark the credential reauth-required here; the original
        // policy or credential row may already be gone.
        let mut cache = self.cache.lock().await;
        let Some(refresh_token_ciphertext) = cache.refresh_token_ciphertext.as_deref() else {
            tracing::debug!(
                policy_id = self.policy_id,
                cloud = ?self.cloud,
                tenant = %self.tenant,
                "Microsoft Graph cleanup token refresh skipped because refresh token is missing"
            );
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "storage cleanup credential is missing refresh token; reauthorize Microsoft Graph",
            ));
        };
        let refresh_token = decrypt_oauth_token(
            &self.encryption_key,
            self.policy_id,
            "refresh",
            refresh_token_ciphertext,
        )?;
        let token = self
            .token_refresher
            .refresh_token(MicrosoftGraphTokenRefreshRequest {
                cloud: self.cloud,
                tenant: self.tenant.clone(),
                client_id: self.client_id.clone(),
                client_secret: self.client_secret.clone(),
                refresh_token: SecretString::from(refresh_token),
            })
            .await
            .map_err(|error| {
                let kind = error.storage_error_kind().unwrap_or(StorageErrorKind::Auth);
                tracing::warn!(
                    policy_id = self.policy_id,
                    cloud = ?self.cloud,
                    tenant = %self.tenant,
                    error = %error,
                    "Microsoft Graph cleanup token refresh failed"
                );
                storage_driver_error(
                    kind,
                    format!("refresh Microsoft Graph cleanup access token: {error}"),
                )
            })?;
        let now = Utc::now();
        cache.access_token = token.access_token;
        cache.expires_at = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| now + Duration::seconds(seconds)));
        if let Some(refresh_token) = token
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|refresh_token| !refresh_token.is_empty())
        {
            cache.refresh_token_ciphertext = Some(crypto::encrypt_token(
                &self.encryption_key,
                crypto::token_aad(
                    self.policy_id,
                    StorageCredentialProvider::MicrosoftGraph.as_str(),
                    "refresh",
                )
                .as_bytes(),
                refresh_token,
            )?);
            tracing::warn!(
                policy_id = self.policy_id,
                cloud = ?self.cloud,
                tenant = %self.tenant,
                "Microsoft Graph cleanup refresh token rotated in memory only"
            );
        }
        Ok(cache.access_token.clone())
    }
}

impl MicrosoftGraphCredentialTokenProvider {
    async fn persist_cache_if_revision(
        &self,
        cache: &MicrosoftGraphConnectorCredentialCache,
        expected_revision: i64,
    ) -> Result<bool> {
        let plaintext = serde_json::to_string(&cache.credential).map_err(|error| {
            AsterError::internal_error(format!("serialize OneDrive connector credential: {error}"))
        })?;
        let ciphertext = crypto::encrypt_connector_credential(
            &self.encryption_key,
            self.policy_id,
            super::OneDriveConnector::ID,
            1,
            &plaintext,
        )?;
        storage_policy_connector_credential_repo::update_if_revision(
            &self.db,
            self.policy_id,
            super::OneDriveConnector::ID,
            1,
            expected_revision,
            ciphertext,
        )
        .await
    }

    async fn recover_from_concurrent_refresh(
        &self,
        cache: &mut MicrosoftGraphConnectorCredentialCache,
        used_revision: i64,
    ) -> Result<Option<String>> {
        let Some(credential) =
            storage_policy_connector_credential_repo::find_by_policy(&self.db, self.policy_id)
                .await?
        else {
            return Ok(None);
        };
        if credential.revision == used_revision {
            return Ok(None);
        }
        let payload: super::OneDriveCredentialV1 =
            crate::storage::connectors::decode_typed_connector_credential(
                &self.encryption_key,
                &credential,
                &aster_drive_storage::ConnectorId::declared(super::OneDriveConnector::ID),
                1,
            )?;
        let Some(authorization) = payload.authorization.as_ref() else {
            return Ok(None);
        };
        if authorization.access_token.trim().is_empty() {
            return Ok(None);
        }
        let access_token = authorization.access_token.clone();
        let expires_at = authorization.expires_at;
        cache.credential = payload;
        cache.revision = credential.revision;
        if cached_access_token_is_fresh(expires_at) {
            return Ok(Some(access_token));
        }

        Err(storage_driver_error(
            StorageErrorKind::Auth,
            "Microsoft Graph refresh token was already rotated; retry the request with the latest credential state",
        ))
    }

    async fn mark_reauth_required_locked(
        &self,
        cache: &mut MicrosoftGraphConnectorCredentialCache,
        reason: &str,
    ) -> Result<()> {
        let Some(authorization) = cache.credential.authorization.as_mut() else {
            return Ok(());
        };
        authorization.status = StorageCredentialStatus::ReauthRequired;
        authorization.status_reason = Some(reason.to_string());
        let expected_revision = cache.revision;
        if self
            .persist_cache_if_revision(cache, expected_revision)
            .await?
        {
            cache.revision = cache.revision.checked_add(1).ok_or_else(|| {
                AsterError::database_operation("storage connector credential revision overflow")
            })?;
        }
        let application = &cache.credential.application;
        let fields =
            microsoft_graph_audit_fields(application.cloud, &application.tenant, None, None);
        write_storage_credential_oauth_audit(
            &self.db,
            0,
            StorageCredentialOauthAuditDetails {
                event: OAUTH_AUDIT_EVENT_REAUTH_REQUIRED,
                result: OAUTH_AUDIT_RESULT_FAILED,
                policy_id: Some(self.policy_id),
                connector_id: Some(super::OneDriveConnector::ID),
                provider: Some(StorageCredentialProvider::MicrosoftGraph),
                fields: Some(&fields),
                reason: Some(reason),
            },
        )
        .await;
        Ok(())
    }
}

fn cached_access_token_is_fresh(expires_at: Option<chrono::DateTime<Utc>>) -> bool {
    expires_at.is_some_and(|expires_at| expires_at > Utc::now() + Duration::seconds(60))
}

fn microsoft_graph_audit_fields(
    cloud: MicrosoftGraphCloud,
    tenant: &str,
    refresh_token_rotated: Option<bool>,
    recovered_from_token_rotation: Option<bool>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "cloud".to_string(),
        serde_json::to_value(cloud).unwrap_or(serde_json::Value::Null),
    );
    fields.insert(
        "tenant".to_string(),
        serde_json::Value::String(tenant.to_string()),
    );
    if let Some(value) = refresh_token_rotated {
        fields.insert(
            "refresh_token_rotated".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    if let Some(value) = recovered_from_token_rotation {
        fields.insert(
            "recovered_from_token_rotation".to_string(),
            serde_json::Value::Bool(value),
        );
    }
    fields
}
