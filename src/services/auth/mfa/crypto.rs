//! MFA secret 加密与 token hash。

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::utils::hash;

const SECRET_CIPHERTEXT_VERSION: &str = "v1";
const MFA_SECRET_INFO: &[u8] = b"asterdrive:mfa-secret:v1";

pub fn token_hash(token: &str) -> String {
    hash::sha256_hex(token.as_bytes())
}

fn cipher(master_key: &str) -> Result<Aes256Gcm> {
    let hk = Hkdf::<Sha256>::new(None, master_key.as_bytes());
    let mut key = [0_u8; 32];
    hk.expand(MFA_SECRET_INFO, &mut key).map_aster_err_ctx(
        "failed to derive MFA encryption key",
        AsterError::config_error,
    )?;
    Aes256Gcm::new_from_slice(&key)
        .map_aster_err_ctx("invalid MFA encryption key", AsterError::config_error)
}

pub fn encrypt_secret(master_key: &str, aad: &[u8], plaintext: &[u8]) -> Result<String> {
    let cipher = cipher(master_key)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_aster_err_ctx("failed to encrypt MFA secret", AsterError::internal_error)?;
    Ok(format!(
        "{}:{}:{}",
        SECRET_CIPHERTEXT_VERSION,
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

pub fn decrypt_secret(master_key: &str, aad: &[u8], ciphertext: &str) -> Result<Vec<u8>> {
    let mut parts = ciphertext.split(':');
    let version = parts.next();
    let nonce = parts.next();
    let encrypted = parts.next();
    if version != Some(SECRET_CIPHERTEXT_VERSION)
        || nonce.is_none()
        || encrypted.is_none()
        || parts.next().is_some()
    {
        return Err(AsterError::database_operation(
            "invalid MFA secret ciphertext format",
        ));
    }

    let nonce = nonce
        .ok_or_else(|| AsterError::database_operation("invalid MFA secret ciphertext format"))?;
    let encrypted = encrypted
        .ok_or_else(|| AsterError::database_operation("invalid MFA secret ciphertext format"))?;

    let nonce = URL_SAFE_NO_PAD
        .decode(nonce)
        .map_aster_err_ctx("invalid MFA secret nonce", AsterError::database_operation)?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| AsterError::database_operation("invalid MFA secret nonce length"))?;
    let encrypted = URL_SAFE_NO_PAD.decode(encrypted).map_aster_err_ctx(
        "invalid MFA secret ciphertext",
        AsterError::database_operation,
    )?;
    let nonce = Nonce::from_slice(&nonce);
    cipher(master_key)?
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: encrypted.as_slice(),
                aad,
            },
        )
        .map_aster_err_ctx(
            "failed to decrypt MFA secret",
            AsterError::database_operation,
        )
}

pub fn factor_aad(user_id: i64, method: &str) -> String {
    format!("mfa_factor:{user_id}:{method}")
}

pub fn setup_flow_aad(user_id: i64) -> String {
    format!("mfa_totp_setup:{user_id}")
}
