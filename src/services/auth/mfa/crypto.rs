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
    use super::{MFA_SECRET_INFO, decrypt_secret, encrypt_secret, factor_aad};

    const TEST_MASTER_KEY: &str = "forge-secret-envelope-test-master-key";
    const TEST_SECRET: &[u8] = b"JBSWY3DPEHPK3PXP";

    #[test]
    fn mfa_secret_round_trips_only_with_matching_factor_aad() {
        let aad = factor_aad(7, "totp");
        let ciphertext = encrypt_secret(TEST_MASTER_KEY, aad.as_bytes(), TEST_SECRET)
            .expect("MFA secret should encrypt");

        assert_eq!(
            decrypt_secret(TEST_MASTER_KEY, aad.as_bytes(), &ciphertext)
                .expect("matching MFA factor AAD should decrypt"),
            TEST_SECRET
        );
        assert!(
            decrypt_secret(
                TEST_MASTER_KEY,
                factor_aad(8, "totp").as_bytes(),
                &ciphertext,
            )
            .is_err(),
            "another user's factor AAD must not decrypt the secret"
        );
        assert!(
            decrypt_secret(
                TEST_MASTER_KEY,
                factor_aad(7, "recovery_code").as_bytes(),
                &ciphertext,
            )
            .is_err(),
            "another factor method's AAD must not decrypt the secret"
        );
    }

    #[test]
    fn existing_mfa_v1_fixture_remains_readable_without_migration() {
        let plaintext = decrypt_secret(
            TEST_MASTER_KEY,
            b"mfa_factor:7:totp",
            "v1:AAECAwQFBgcICQoL:pt1VIrNAcBeWaV0OT5oopWy1VJSEeAF3WeFu3yRJ_EE",
        )
        .expect("existing MFA v1 fixture should decrypt");

        assert_eq!(MFA_SECRET_INFO, b"asterdrive:mfa-secret:v1");
        assert_eq!(plaintext, TEST_SECRET);
    }
}
