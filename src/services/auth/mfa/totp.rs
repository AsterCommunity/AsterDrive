//! TOTP 生成与校验。

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use rand::RngExt;
use sha1::Sha1;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::utils::numbers::{i64_to_u64, u64_to_usize};

const TOTP_SECRET_BYTES: usize = 20;
const TOTP_STEP_SECS: u64 = 30;
const TOTP_DIGITS: u32 = 6;
const TOTP_WINDOW_STEPS: i64 = 1;

type HmacSha1 = Hmac<Sha1>;

pub fn generate_secret() -> Vec<u8> {
    let mut secret = vec![0_u8; TOTP_SECRET_BYTES];
    let mut rng = rand::rng();
    rng.fill(&mut secret[..]);
    secret
}

pub fn encode_secret(secret: &[u8]) -> String {
    BASE32_NOPAD.encode(secret)
}

pub fn decode_secret(secret: &str) -> Result<Vec<u8>> {
    BASE32_NOPAD
        .decode(secret.trim().to_ascii_uppercase().as_bytes())
        .map_err(|error| AsterError::validation_error(format!("invalid TOTP secret: {error}")))
}

pub fn otpauth_uri(secret_base32: &str, issuer: &str, account: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        urlencoding::encode(issuer),
        urlencoding::encode(account),
        urlencoding::encode(secret_base32),
        urlencoding::encode(issuer),
        TOTP_DIGITS,
        TOTP_STEP_SECS
    )
}

pub fn verify_code(secret: &[u8], code: &str, now: chrono::DateTime<chrono::Utc>) -> Result<bool> {
    let normalized = normalize_code(code)?;
    let timestamp = i64_to_u64(now.timestamp(), "totp timestamp")?;
    let current_step = timestamp / TOTP_STEP_SECS;
    for offset in -TOTP_WINDOW_STEPS..=TOTP_WINDOW_STEPS {
        let step = if offset < 0 {
            current_step.saturating_sub(offset.unsigned_abs())
        } else {
            current_step.saturating_add(offset.unsigned_abs())
        };
        if totp_code(secret, step)? == normalized {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn looks_like_code(code: &str) -> bool {
    let normalized = code.trim().replace(' ', "");
    normalized.len() == TOTP_DIGITS as usize && normalized.chars().all(|c| c.is_ascii_digit())
}

pub fn code_for_time(secret: &[u8], now: chrono::DateTime<chrono::Utc>) -> Result<String> {
    let timestamp = i64_to_u64(now.timestamp(), "totp timestamp")?;
    totp_code(secret, timestamp / TOTP_STEP_SECS)
}

fn normalize_code(code: &str) -> Result<String> {
    let normalized = code.trim().replace(' ', "");
    if !looks_like_code(&normalized) {
        return Err(AsterError::validation_error(
            "TOTP code must be a 6 digit number",
        ));
    }
    Ok(normalized)
}

fn totp_code(secret: &[u8], step: u64) -> Result<String> {
    let mut mac = <HmacSha1 as KeyInit>::new_from_slice(secret)
        .map_aster_err_ctx("invalid TOTP secret", AsterError::internal_error)?;
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let binary = ((u32::from(digest[offset]) & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    let modulus = 10_u32.pow(TOTP_DIGITS);
    let code = binary % modulus;
    let width = u64_to_usize(u64::from(TOTP_DIGITS), "totp digits")?;
    Ok(format!("{code:0width$}"))
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    #[test]
    fn rfc6238_sha1_test_vector_round_trips() {
        let secret = b"12345678901234567890";
        let timestamp = DateTime::<Utc>::from_timestamp(59, 0).unwrap();
        assert!(verify_code(secret, "287082", timestamp).unwrap());
        assert!(!verify_code(secret, "000000", timestamp).unwrap());
    }
}
