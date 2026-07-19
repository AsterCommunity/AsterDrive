use serde::{Deserialize, Serialize};

use crate::entities::upload_session;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::storage_policy::credential::crypto;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProviderSessionSecret {
    pub(super) provider: String,
    pub(super) upload_url: String,
}

fn provider_session_aad(upload_id: &str) -> String {
    format!("upload_session:{upload_id}:provider_resumable")
}

pub(super) fn encrypt_provider_session(
    state: &impl SharedRuntimeState,
    upload_id: &str,
    secret: &ProviderSessionSecret,
) -> Result<String> {
    let plaintext = serde_json::to_string(secret).map_err(|error| {
        AsterError::internal_error(format!("serialize provider upload session: {error}"))
    })?;
    crypto::encrypt_token(
        &state.config().auth.storage_credential_secret_key,
        provider_session_aad(upload_id).as_bytes(),
        &plaintext,
    )
}

pub(super) fn decrypt_provider_session(
    state: &impl SharedRuntimeState,
    session: &upload_session::Model,
) -> Result<ProviderSessionSecret> {
    let ciphertext = session
        .provider_session_ciphertext
        .as_deref()
        .ok_or_else(|| {
            AsterError::database_operation("provider upload session metadata is missing")
        })?;
    let plaintext = crypto::decrypt_token(
        &state.config().auth.storage_credential_secret_key,
        provider_session_aad(&session.id).as_bytes(),
        ciphertext,
    )?;
    serde_json::from_str(&plaintext).map_err(|error| {
        AsterError::database_operation(format!(
            "provider upload session metadata is invalid: {error}"
        ))
    })
}
