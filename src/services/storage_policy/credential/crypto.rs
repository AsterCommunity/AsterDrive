//! Storage policy credential token encryption and flow token hashing.

use crate::errors::{AsterError, MapAsterErr, Result};
use aster_forge_crypto as hash;
use serde::{Deserialize, Serialize};

const STORAGE_CREDENTIAL_INFO: &[u8] = b"asterdrive:storage-credential-token:v1";
const MIN_MASTER_KEY_LEN: usize = 32;
const CONNECTOR_CREDENTIAL_FORMAT_VERSION: u32 = 1;
const CONNECTOR_CREDENTIAL_AAD_PREFIX: &str = "storage_policy_connector_credential";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCredentialCiphertextEnvelope {
    pub format_version: u32,
    pub connector_id: String,
    pub schema_version: u32,
    pub ciphertext: String,
}

pub fn token_hash(token: &str) -> String {
    hash::sha256_hex(token.as_bytes())
}

fn validate_master_key(master_key: &str) -> Result<&str> {
    let master_key = master_key.trim();
    if master_key.len() < MIN_MASTER_KEY_LEN {
        return Err(AsterError::config_error(format!(
            "storage credential encryption master key must be at least {MIN_MASTER_KEY_LEN} characters"
        )));
    }
    Ok(master_key)
}

pub fn encrypt_token(master_key: &str, aad: &[u8], plaintext: &str) -> Result<String> {
    let master_key = validate_master_key(master_key)?;
    aster_forge_crypto::encrypt_secret(
        master_key.as_bytes(),
        STORAGE_CREDENTIAL_INFO,
        aad,
        plaintext.as_bytes(),
    )
    .map_aster_err_ctx(
        "failed to encrypt storage credential token",
        AsterError::internal_error,
    )
}

pub fn decrypt_token(master_key: &str, aad: &[u8], ciphertext: &str) -> Result<String> {
    let master_key = validate_master_key(master_key)?;
    let plaintext = aster_forge_crypto::decrypt_secret(
        master_key.as_bytes(),
        STORAGE_CREDENTIAL_INFO,
        aad,
        ciphertext,
    )
    .map_aster_err_ctx(
        "failed to decrypt storage credential token",
        AsterError::database_operation,
    )?;
    String::from_utf8(plaintext).map_aster_err_ctx(
        "storage credential token plaintext is not UTF-8",
        AsterError::database_operation,
    )
}

pub fn token_aad(policy_id: i64, provider: &str, token_name: &str) -> String {
    format!("storage_policy_credential:{policy_id}:{provider}:{token_name}")
}

pub fn connector_credential_aad(policy_id: i64, connector_id: &str, schema_version: u32) -> String {
    format!("{CONNECTOR_CREDENTIAL_AAD_PREFIX}:{policy_id}:{connector_id}:{schema_version}")
}

pub fn encrypt_connector_credential(
    master_key: &str,
    policy_id: i64,
    connector_id: &str,
    schema_version: u32,
    plaintext: &str,
) -> Result<String> {
    let inner = encrypt_token(
        master_key,
        connector_credential_aad(policy_id, connector_id, schema_version).as_bytes(),
        plaintext,
    )?;
    serde_json::to_string(&ConnectorCredentialCiphertextEnvelope {
        format_version: CONNECTOR_CREDENTIAL_FORMAT_VERSION,
        connector_id: connector_id.to_string(),
        schema_version,
        ciphertext: inner,
    })
    .map_aster_err_ctx(
        "serialize storage connector credential ciphertext envelope",
        AsterError::internal_error,
    )
}

pub fn decrypt_connector_credential(
    master_key: &str,
    policy_id: i64,
    connector_id: &str,
    schema_version: u32,
    raw: &str,
) -> Result<String> {
    let envelope: ConnectorCredentialCiphertextEnvelope = serde_json::from_str(raw)
        .map_aster_err_ctx(
            "invalid storage connector credential ciphertext envelope",
            AsterError::database_operation,
        )?;
    if envelope.format_version != CONNECTOR_CREDENTIAL_FORMAT_VERSION
        || envelope.connector_id != connector_id
        || envelope.schema_version != schema_version
    {
        return Err(AsterError::database_operation(
            "storage connector credential ciphertext envelope does not match connector schema",
        ));
    }
    decrypt_token(
        master_key,
        connector_credential_aad(policy_id, connector_id, schema_version).as_bytes(),
        &envelope.ciphertext,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_ciphertext_round_trips_with_matching_aad() {
        let key = "storage-token-test-master-key-32bytes";
        let aad = token_aad(7, "microsoft_graph", "access");
        let encrypted = encrypt_token(key, aad.as_bytes(), "secret-token").unwrap();

        assert_ne!(encrypted, "secret-token");
        assert_eq!(
            decrypt_token(key, aad.as_bytes(), &encrypted).unwrap(),
            "secret-token"
        );
    }

    #[test]
    fn token_ciphertext_rejects_wrong_aad() {
        let key = "storage-token-test-master-key-32bytes";
        let encrypted = encrypt_token(key, b"aad-one", "secret-token").unwrap();

        assert!(decrypt_token(key, b"aad-two", &encrypted).is_err());
    }

    #[test]
    fn token_ciphertext_rejects_short_master_key() {
        let error = encrypt_token("short", b"aad", "secret-token").unwrap_err();

        assert!(error.to_string().contains("at least 32 characters"));
    }

    #[test]
    fn connector_credential_envelope_binds_policy_connector_and_schema() {
        let key = "storage-static-test-master-key-32bytes";
        let encrypted =
            encrypt_connector_credential(key, 7, "asterdrive.storage.s3", 1, "{\"x\":1}").unwrap();
        assert!(encrypted.contains("format_version"));
        assert_eq!(
            decrypt_connector_credential(key, 7, "asterdrive.storage.s3", 1, &encrypted).unwrap(),
            "{\"x\":1}"
        );
        assert!(
            decrypt_connector_credential(key, 8, "asterdrive.storage.s3", 1, &encrypted).is_err()
        );
        assert!(
            decrypt_connector_credential(key, 7, "asterdrive.storage.sftp", 1, &encrypted).is_err()
        );
        assert!(
            decrypt_connector_credential(key, 7, "asterdrive.storage.s3", 2, &encrypted).is_err()
        );
    }

    #[test]
    fn existing_storage_v1_fixture_remains_readable_without_migration() {
        let plaintext = decrypt_token(
            "forge-secret-envelope-test-master-key",
            b"storage_policy_credential:9:microsoft_graph:access",
            "v1:AAECAwQFBgcICQoL:lVf2A9KFG97Bm8ru8l9wF-i_taTNtAbZ-MYT3Kujr95sOUE",
        )
        .expect("existing storage credential v1 fixture should decrypt");

        assert_eq!(plaintext, "opaque-access-token");
    }
}
