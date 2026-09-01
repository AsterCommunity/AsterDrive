use serde::{Deserialize, Serialize};

use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::storage_policy::credential::crypto;
use aster_drive_model::entities::upload_session;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProviderSessionSecret {
    pub(crate) provider: String,
    pub(crate) upload_url: String,
}

fn provider_session_aad(upload_id: &str) -> String {
    format!("upload_session:{upload_id}:provider_resumable")
}

pub(crate) fn encrypt_provider_session(
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

pub(crate) fn decrypt_provider_session(
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
