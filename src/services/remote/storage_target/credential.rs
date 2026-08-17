use crate::errors::{AsterError, MapAsterErr, Result};
use serde::{Deserialize, Serialize};

const INFO: &[u8] = b"asterdrive:remote-storage-target-credential:v1";
const FORMAT_VERSION: u32 = 1;
const MIN_MASTER_KEY_LEN: usize = 32;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CiphertextEnvelope {
    format_version: u32,
    connector_id: String,
    schema_version: u32,
    ciphertext: String,
}

fn aad(target_id: i64, connector_id: &str, schema_version: u32) -> String {
    format!("remote_storage_target_credential:{target_id}:{connector_id}:{schema_version}")
}

fn key(master_key: &str) -> Result<&str> {
    let key = master_key.trim();
    if key.len() < MIN_MASTER_KEY_LEN {
        return Err(AsterError::config_error(format!(
            "storage credential encryption master key must be at least {MIN_MASTER_KEY_LEN} characters"
        )));
    }
    Ok(key)
}

pub(super) fn encrypt(
    master_key: &str,
    target_id: i64,
    connector_id: &str,
    schema_version: u32,
    plaintext: &str,
) -> Result<String> {
    let inner = aster_forge_crypto::encrypt_secret(
        key(master_key)?.as_bytes(),
        INFO,
        aad(target_id, connector_id, schema_version).as_bytes(),
        plaintext.as_bytes(),
    )
    .map_aster_err_ctx(
        "encrypt remote target credential",
        AsterError::internal_error,
    )?;
    serde_json::to_string(&CiphertextEnvelope {
        format_version: FORMAT_VERSION,
        connector_id: connector_id.to_string(),
        schema_version,
        ciphertext: inner,
    })
    .map_aster_err_ctx(
        "serialize remote target credential",
        AsterError::internal_error,
    )
}

pub(super) fn decrypt(
    master_key: &str,
    target_id: i64,
    connector_id: &str,
    schema_version: u32,
    raw: &str,
) -> Result<String> {
    let envelope: CiphertextEnvelope = serde_json::from_str(raw).map_aster_err_ctx(
        "invalid remote target credential envelope",
        AsterError::database_operation,
    )?;
    if envelope.format_version != FORMAT_VERSION
        || envelope.connector_id != connector_id
        || envelope.schema_version != schema_version
    {
        return Err(AsterError::database_operation(
            "remote target credential envelope does not match connector schema",
        ));
    }
    let plaintext = aster_forge_crypto::decrypt_secret(
        key(master_key)?.as_bytes(),
        INFO,
        aad(target_id, connector_id, schema_version).as_bytes(),
        &envelope.ciphertext,
    )
    .map_aster_err_ctx(
        "decrypt remote target credential",
        AsterError::database_operation,
    )?;
    String::from_utf8(plaintext).map_aster_err_ctx(
        "remote target credential plaintext is not UTF-8",
        AsterError::database_operation,
    )
}
