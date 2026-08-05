//! MFA secret 加密与 token hash。

use crate::errors::{AsterError, MapAsterErr, Result};
use aster_forge_crypto as hash;

const MFA_SECRET_INFO: &[u8] = b"asterdrive:mfa-secret:v1";

pub fn token_hash(token: &str) -> String {
    hash::sha256_hex(token.as_bytes())
}

pub fn encrypt_secret(master_key: &str, aad: &[u8], plaintext: &[u8]) -> Result<String> {
    aster_forge_crypto::encrypt_secret(master_key.as_bytes(), MFA_SECRET_INFO, aad, plaintext)
        .map_aster_err_ctx("failed to encrypt MFA secret", AsterError::internal_error)
}

pub fn decrypt_secret(master_key: &str, aad: &[u8], ciphertext: &str) -> Result<Vec<u8>> {
    aster_forge_crypto::decrypt_secret(master_key.as_bytes(), MFA_SECRET_INFO, aad, ciphertext)
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

#[cfg(test)]
mod tests {
    use super::{MFA_SECRET_INFO, decrypt_secret};

    #[test]
    fn existing_mfa_v1_fixture_remains_readable_without_migration() {
        let plaintext = decrypt_secret(
            "forge-secret-envelope-test-master-key",
            b"mfa_factor:7:totp",
            "v1:AAECAwQFBgcICQoL:pt1VIrNAcBeWaV0OT5oopWy1VJSEeAF3WeFu3yRJ_EE",
        )
        .expect("existing MFA v1 fixture should decrypt");

        assert_eq!(MFA_SECRET_INFO, b"asterdrive:mfa-secret:v1");
        assert_eq!(plaintext, b"JBSWY3DPEHPK3PXP");
    }
}
