//! Policy-bound OAuth flow. Product persistence stays in the connector.
//! <https://developers.google.com/identity/protocols/oauth2/web-server>
use super::*;

pub(super) fn authorization_url(
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
    challenge: &str,
) -> Result<String> {
    reqwest::Url::parse_with_params(
        GOOGLE_AUTH,
        [
            ("client_id", client_id),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("scope", scope),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ],
    )
    .map(|u| u.to_string())
    .map_err(|_| AsterError::validation_error("invalid Google OAuth parameters"))
}
impl GoogleDriveConnector {
    pub(super) async fn complete_authorization(
        &self,
        context: &crate::storage::connectors::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        code: &str,
        now: chrono::DateTime<Utc>,
        token_endpoint: &str,
    ) -> Result<StorageConnectorAuthorizationCallback> {
        if flow.provider != StorageCredentialProvider::GoogleDrive
            || flow.policy_id != Some(policy.id)
        {
            return Err(AsterError::validation_error(
                "Google OAuth flow does not belong to this policy",
            ));
        }
        let f: GoogleDriveFlowContext = serde_json::from_str(&flow.context)
            .map_err(|_| AsterError::database_operation("invalid Google OAuth flow context"))?;
        let key = &context.config().auth.storage_credential_secret_key;
        let plaintext = crypto::decrypt_token(
            key,
            flow_aad(policy.id, &flow.state_hash).as_bytes(),
            &f.application_ciphertext,
        )?;
        let app: GoogleDriveAuthorizationApplicationV1 =
            serde_json::from_str(&plaintext).map_err(|_| {
                AsterError::database_operation("invalid Google OAuth application context")
            })?;
        let verifier = flow
            .pkce_verifier
            .as_deref()
            .ok_or_else(|| AsterError::validation_error("Google OAuth PKCE verifier is missing"))?;
        // Fail stale configuration before sending the one-time code, and check
        // again after network I/O. The final persistence also uses this revision
        // so an administrator edit cannot be overwritten in the commit window.
        let current = repo::find_by_policy(context.writer_db(), policy.id)
            .await?
            .ok_or_else(|| AsterError::record_not_found("Google Drive credential"))?;
        if current.revision != f.credential_revision
            || scope(&Self::decode_config(policy)?) != f.scope
        {
            return Err(AsterError::validation_error(
                "Google Drive configuration changed during authorization; restart authorization",
            ));
        }
        if code.trim().is_empty() || verifier.trim().is_empty() {
            return Err(AsterError::validation_error(
                "Google OAuth code and PKCE verifier are required",
            ));
        }
        let response = http_client()?
            .post(token_endpoint)
            .timeout(std::time::Duration::from_secs(30))
            .form(&[
                ("code", code),
                ("client_id", app.client_id.as_str()),
                ("client_secret", app.client_secret.as_str()),
                ("redirect_uri", flow.redirect_uri.as_str()),
                ("code_verifier", verifier),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    "Google OAuth token endpoint is unavailable",
                )
            })?;
        if !response.status().is_success() {
            let failure = provider::classify_token_failure(response).await;
            return Err(storage_driver_error(failure.kind, failure.reason));
        }
        let token: provider::TokenResponse = response.json().await.map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Transient,
                "invalid Google OAuth token response",
            )
        })?;
        token.validate()?;
        let current = repo::find_by_policy(context.writer_db(), policy.id)
            .await?
            .ok_or_else(|| AsterError::record_not_found("Google Drive credential"))?;
        if current.revision != f.credential_revision
            || scope(&Self::decode_config(policy)?) != f.scope
        {
            return Err(AsterError::validation_error(
                "Google Drive configuration changed during authorization; restart authorization",
            ));
        }
        let expires_at = token.expires_at(now);
        let scopes: Vec<String> = token
            .scope
            .as_deref()
            .unwrap_or(&f.scope)
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if !scopes.contains(&f.scope) {
            return Err(storage_driver_error(
                StorageErrorKind::Permission,
                "Google OAuth did not grant the requested Drive scope",
            ));
        }
        let refresh_token = token
            .refresh_token
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    "Google OAuth did not return an offline refresh token; reauthorize",
                )
            })?;
        let payload = GoogleDriveCredentialV1 {
            application: app,
            authorization: Some(GoogleDriveAuthorizationV1 {
                access_token: token.access_token,
                refresh_token: Some(refresh_token),
                scopes,
                status: StorageCredentialStatus::Authorized,
                status_reason: None,
                expires_at,
                authorized_at: Some(now),
                last_refreshed_at: None,
                last_validated_at: None,
            }),
        };
        Ok(StorageConnectorAuthorizationCallback {
            credential_payload: encode(&payload)?,
            audit: audit(),
            expected_credential_revision: f.credential_revision,
        })
    }
}
