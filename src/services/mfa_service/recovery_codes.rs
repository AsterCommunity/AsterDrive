//! MFA 恢复码生成与验证。

use rand::RngExt;
use sea_orm::ActiveValue::Set;

use crate::db::repository::mfa_recovery_code_repo;
use crate::entities::mfa_recovery_code;
use crate::errors::{AsterError, Result};
use crate::utils::hash;

use super::{RECOVERY_CODE_CHARS, RECOVERY_CODE_COUNT, now_utc};

const RECOVERY_CODE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const RECOVERY_CODE_GROUPS: usize = 3;

pub struct GeneratedRecoveryCodes {
    pub plaintext: Vec<String>,
    pub models: Vec<mfa_recovery_code::ActiveModel>,
}

pub fn generate_for_user(user_id: i64) -> Result<GeneratedRecoveryCodes> {
    let now = now_utc();
    let mut plaintext = Vec::with_capacity(RECOVERY_CODE_COUNT);
    let mut models = Vec::with_capacity(RECOVERY_CODE_COUNT);
    for _ in 0..RECOVERY_CODE_COUNT {
        let code = generate_code();
        let normalized = normalize_code(&code)?;
        let code_hash = hash::hash_password(&normalized)?;
        plaintext.push(code);
        models.push(mfa_recovery_code::ActiveModel {
            user_id: Set(user_id),
            code_hash: Set(code_hash),
            used_at: Set(None),
            created_at: Set(now),
            ..Default::default()
        });
    }
    Ok(GeneratedRecoveryCodes { plaintext, models })
}

pub async fn replace_for_user<C: sea_orm::ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<String>> {
    let generated = generate_for_user(user_id)?;
    mfa_recovery_code_repo::delete_all_for_user(db, user_id).await?;
    mfa_recovery_code_repo::create_many(db, generated.models).await?;
    Ok(generated.plaintext)
}

pub async fn verify_and_consume<C: sea_orm::ConnectionTrait>(
    db: &C,
    user_id: i64,
    code: &str,
) -> Result<bool> {
    let code = normalize_code(code)?;
    let unused = mfa_recovery_code_repo::list_unused_for_user(db, user_id).await?;
    for item in unused {
        if hash::verify_password(&code, &item.code_hash)? {
            return mfa_recovery_code_repo::mark_used(db, item.id, now_utc()).await;
        }
    }
    Ok(false)
}

pub fn looks_like_code(code: &str) -> bool {
    let normalized = normalize_code_text(code);
    normalized.len() >= 8 && normalized.chars().all(|c| c.is_ascii_alphanumeric())
}

fn generate_code() -> String {
    let mut rng = rand::rng();
    let raw = (0..RECOVERY_CODE_CHARS)
        .map(|_| {
            let idx = rng.random_range(0..RECOVERY_CODE_ALPHABET.len());
            RECOVERY_CODE_ALPHABET[idx] as char
        })
        .collect::<Vec<_>>();
    let group_size = RECOVERY_CODE_CHARS.div_ceil(RECOVERY_CODE_GROUPS).max(1);
    raw.chunks(group_size)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

fn normalize_code(code: &str) -> Result<String> {
    let normalized = normalize_code_text(code);
    if !looks_like_code(&normalized) {
        return Err(AsterError::validation_error("invalid recovery code format"));
    }
    Ok(normalized)
}

fn normalize_code_text(code: &str) -> String {
    code.trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase()
}
