//! Storage policy credential token encryption and flow token hashing.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::utils::hash;

const CIPHERTEXT_VERSION: &str = "v1";
const STORAGE_CREDENTIAL_INFO: &[u8] = b"asterdrive:storage-credential-token:v1";
const MIN_MASTER_KEY_LEN: usize = 32;

pub fn token_hash(token: &str) -> String {
    hash::sha256_hex(token.as_bytes())
}

fn cipher(master_key: &str) -> Result<Aes256Gcm> {
    let master_key = master_key.trim();
    if master_key.len() < MIN_MASTER_KEY_LEN {
        return Err(AsterError::config_error(format!(
            "storage credential encryption master key must be at least {MIN_MASTER_KEY_LEN} characters"
        )));
    }
    let hk = Hkdf::<Sha256>::new(None, master_key.as_bytes());
    let mut key = [0_u8; 32];
    hk.expand(STORAGE_CREDENTIAL_INFO, &mut key)
        .map_aster_err_ctx(
            "failed to derive storage credential encryption key",
            AsterError::config_error,
        )?;
    Aes256Gcm::new_from_slice(&key).map_aster_err_ctx(
        "invalid storage credential encryption key",
        AsterError::config_error,
    )
}

pub fn encrypt_token(master_key: &str, aad: &[u8], plaintext: &str) -> Result<String> {
    let cipher = cipher(master_key)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext.as_bytes(),
                aad,
            },
        )
        .map_aster_err_ctx(
            "failed to encrypt storage credential token",
            AsterError::internal_error,
        )?;
    Ok(format!(
        "{}:{}:{}",
        CIPHERTEXT_VERSION,
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

pub fn decrypt_token(master_key: &str, aad: &[u8], ciphertext: &str) -> Result<String> {
    let mut parts = ciphertext.split(':');
    let version = parts.next();
    let nonce = parts.next();
    let encrypted = parts.next();
    if version != Some(CIPHERTEXT_VERSION)
        || nonce.is_none()
        || encrypted.is_none()
        || parts.next().is_some()
    {
        return Err(AsterError::database_operation(
            "invalid storage credential token ciphertext format",
        ));
    }

    let nonce = nonce.ok_or_else(|| {
        AsterError::database_operation("invalid storage credential token ciphertext format")
    })?;
    let encrypted = encrypted.ok_or_else(|| {
        AsterError::database_operation("invalid storage credential token ciphertext format")
    })?;

    let nonce = URL_SAFE_NO_PAD.decode(nonce).map_aster_err_ctx(
        "invalid storage credential token nonce",
        AsterError::database_operation,
    )?;
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| {
        AsterError::database_operation("invalid storage credential token nonce length")
    })?;
    let encrypted = URL_SAFE_NO_PAD.decode(encrypted).map_aster_err_ctx(
        "invalid storage credential token ciphertext",
        AsterError::database_operation,
    )?;
    let nonce = Nonce::from_slice(&nonce);
    let plaintext = cipher(master_key)?
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: encrypted.as_slice(),
                aad,
            },
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
}
