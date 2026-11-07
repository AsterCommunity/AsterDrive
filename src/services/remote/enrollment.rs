//! 服务模块：`remote::enrollment`。

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::db::repository::{follower_enrollment_session_repo, managed_follower_repo};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::follower_enrollment_session;
use aster_drive_model::types::ResolvedRemoteTransport;
use aster_forge_db::transaction;
use chrono::{Duration, Utc};
use sea_orm::Set;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

const DEFAULT_ENROLLMENT_TTL_MINUTES: i64 = 30;
const ENROLL_COMMAND_BINARY: &str = "aster_drive";
pub const ENROLLMENT_TOKEN_REPLACED_MESSAGE: &str =
    "enrollment token has been replaced by a newer session";
pub const ENROLLMENT_TOKEN_COMPLETED_MESSAGE: &str = "enrollment token has already been completed";
pub const ENROLLMENT_TOKEN_EXPIRED_MESSAGE: &str = "enrollment token has expired";
pub const ENROLLMENT_TOKEN_REDEEMED_MESSAGE: &str = "enrollment token has already been redeemed";
pub const REMOTE_NODE_ENROLLMENT_ALREADY_COMPLETED_MESSAGE: &str =
    "remote node enrollment has already been completed";

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteEnrollmentCommandInfo {
    pub remote_node_id: i64,
    pub remote_node_name: String,
    pub master_url: String,
    pub token: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<Utc>,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteEnrollmentBootstrap {
    pub remote_node_id: i64,
    pub remote_node_name: String,
    pub master_url: String,
    pub access_key: String,
    pub secret_key: String,
    pub is_enabled: bool,
    #[serde(default)]
    pub resolved_transport: ResolvedRemoteTransport,
    #[serde(default = "default_binding_revision")]
    pub desired_revision: i64,
    pub ack_token: String,
}

const fn default_binding_revision() -> i64 {
    1
}

#[derive(Debug, Clone)]
pub struct RemoteEnrollmentAckInfo {
    pub remote_node_id: i64,
    pub remote_node_name: String,
    pub is_enabled: bool,
}

pub async fn create_enrollment_command<S: SharedRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<RemoteEnrollmentCommandInfo> {
    let remote_node = managed_follower_repo::find_by_id(state.writer_db(), remote_node_id).await?;
    if follower_enrollment_session_repo::has_completed_for_managed_follower(
        state.writer_db(),
        remote_node_id,
    )
    .await?
    {
        return Err(AsterError::validation_error(
            REMOTE_NODE_ENROLLMENT_ALREADY_COMPLETED_MESSAGE,
        ));
    }

    let master_url = site_url::public_site_url(state.runtime_config()).ok_or_else(|| {
        validation_error_with_code(
            ApiErrorCode::ConfigPublicSiteUrlRequired,
            "public_site_url must be configured before generating enrollment commands",
        )
    })?;
    let token = format!("enr_{}", aster_forge_utils::id::new_short_token());
    let ack_token = format!("enr_ack_{}", aster_forge_utils::id::new_short_token());
    let token_hash = aster_forge_crypto::sha256_hex(token.as_bytes());
    let ack_token_hash = aster_forge_crypto::sha256_hex(ack_token.as_bytes());
    let expires_at = Utc::now() + Duration::minutes(DEFAULT_ENROLLMENT_TTL_MINUTES);
    let created_at = Utc::now();

    transaction::with_transaction(state.writer_db(), async |txn| {
        follower_enrollment_session_repo::invalidate_pending_for_managed_follower(
            txn,
            remote_node_id,
        )
        .await?;
        follower_enrollment_session_repo::create(
            txn,
            follower_enrollment_session::ActiveModel {
                managed_follower_id: Set(remote_node_id),
                token_hash: Set(token_hash.clone()),
                ack_token_hash: Set(ack_token_hash.clone()),
                expires_at: Set(expires_at),
                redeemed_at: Set(None),
                acked_at: Set(None),
                invalidated_at: Set(None),
                created_at: Set(created_at),
                ..Default::default()
            },
        )
        .await?;
        Ok::<_, AsterError>(())
    })
    .await?;

    Ok(RemoteEnrollmentCommandInfo {
        remote_node_id,
        remote_node_name: remote_node.name.clone(),
        master_url: master_url.clone(),
        token: token.clone(),
        expires_at,
        command: build_enrollment_command(&master_url, &token),
    })
}

pub async fn redeem_enrollment_token<S: SharedRuntimeState>(
    state: &S,
    token: &str,
) -> Result<RemoteEnrollmentBootstrap> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error("token cannot be blank"));
    }

    let token_hash = aster_forge_crypto::sha256_hex(trimmed.as_bytes());
    transaction::with_transaction(state.writer_db(), async |txn| {
        let now = Utc::now();
        let claimed =
            follower_enrollment_session_repo::claim_redeemable_by_token_hash(txn, &token_hash, now)
                .await?;
        let enrollment = follower_enrollment_session_repo::find_by_token_hash(txn, &token_hash)
            .await?
            .ok_or_else(|| AsterError::validation_error("invalid enrollment token"))?;

        if !claimed {
            return Err(redeem_claim_error(&enrollment, now));
        }

        let master_url = site_url::public_site_url(state.runtime_config()).ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::ConfigPublicSiteUrlRequired,
                "public_site_url is not configured on the master node",
            )
        })?;
        let remote_node =
            managed_follower_repo::find_by_id(txn, enrollment.managed_follower_id).await?;

        Ok(RemoteEnrollmentBootstrap {
            remote_node_id: remote_node.id,
            remote_node_name: remote_node.name,
            master_url,
            access_key: remote_node.access_key,
            secret_key: remote_node.secret_key,
            is_enabled: remote_node.is_enabled,
            resolved_transport: remote_node.transport_mode.resolve(&remote_node.base_url),
            desired_revision: remote_node.binding_revision,
            ack_token: format!("enr_ack_{}", enrollment.ack_token_hash),
        })
    })
    .await
}

pub async fn ack_enrollment_token<S: SharedRuntimeState>(
    state: &S,
    ack_token: &str,
) -> Result<RemoteEnrollmentAckInfo> {
    let trimmed = ack_token.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error("ack_token cannot be blank"));
    }

    let ack_token_hash = normalize_ack_token_hash(trimmed);
    transaction::with_transaction(state.writer_db(), async |txn| {
        let enrollment =
            follower_enrollment_session_repo::find_by_ack_token_hash(txn, &ack_token_hash)
                .await?
                .ok_or_else(|| AsterError::validation_error("invalid enrollment ack token"))?;

        if enrollment.invalidated_at.is_some() {
            return Err(AsterError::validation_error(
                "enrollment session has been invalidated",
            ));
        }
        let remote_node =
            managed_follower_repo::find_by_id(txn, enrollment.managed_follower_id).await?;
        if enrollment.acked_at.is_some() {
            return Ok(RemoteEnrollmentAckInfo {
                remote_node_id: remote_node.id,
                remote_node_name: remote_node.name,
                is_enabled: remote_node.is_enabled,
            });
        }
        if enrollment.redeemed_at.is_none() {
            return Err(AsterError::validation_error(
                "enrollment session must be redeemed before ack",
            ));
        }

        if !follower_enrollment_session_repo::mark_acked(txn, enrollment.id).await? {
            return Err(AsterError::validation_error(
                "enrollment session could not be acknowledged",
            ));
        }
        Ok(RemoteEnrollmentAckInfo {
            remote_node_id: remote_node.id,
            remote_node_name: remote_node.name,
            is_enabled: remote_node.is_enabled,
        })
    })
    .await
}

pub fn build_enrollment_command(master_url: &str, token: &str) -> String {
    let quoted_url = format!("'{}'", master_url.replace('\'', "'\"'\"'"));
    let mut delimiter = "ASTER_ENROLLMENT_TOKEN".to_string();
    while token.lines().any(|line| line == delimiter) {
        delimiter.push('_');
    }
    // A quoted heredoc passes the token through stdin without expanding shell
    // syntax or placing the credential in the enrollment process's argv.
    format!(
        "{ENROLL_COMMAND_BINARY} node enroll --master-url {quoted_url} --token-stdin --yes <<'{delimiter}'\n{token}\n{delimiter}"
    )
}

fn normalize_ack_token_hash(value: &str) -> String {
    if let Some(raw_hash) = value.strip_prefix("enr_ack_")
        && raw_hash.len() == 64
        && raw_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return raw_hash.to_ascii_lowercase();
    }

    aster_forge_crypto::sha256_hex(value.as_bytes())
}

fn redeem_claim_error(
    enrollment: &follower_enrollment_session::Model,
    now: chrono::DateTime<Utc>,
) -> AsterError {
    if enrollment.invalidated_at.is_some() {
        return AsterError::validation_error(ENROLLMENT_TOKEN_REPLACED_MESSAGE);
    }
    if enrollment.acked_at.is_some() {
        return AsterError::validation_error(ENROLLMENT_TOKEN_COMPLETED_MESSAGE);
    }
    if enrollment.expires_at <= now {
        return AsterError::validation_error(ENROLLMENT_TOKEN_EXPIRED_MESSAGE);
    }
    if enrollment.redeemed_at.is_some() {
        return AsterError::validation_error(ENROLLMENT_TOKEN_REDEEMED_MESSAGE);
    }

    AsterError::validation_error("enrollment token is no longer redeemable")
}

#[cfg(test)]
mod tests {
    use super::RemoteEnrollmentBootstrap;

    #[cfg(unix)]
    #[test]
    fn generated_enrollment_command_quotes_url_and_passes_secret_only_through_stdin() {
        use std::io::Read;
        let root =
            std::env::temp_dir().join(format!("enrollment-command-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let guard = aster_forge_utils::raii::TempDirGuard::new(root, "enrollment command test");
        let executable = guard.path().join("aster_drive");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$ENROLL_ARGS\"\n/bin/cat > \"$ENROLL_STDIN\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let args = guard.path().join("args");
        let stdin = guard.path().join("stdin");
        let url = "https://example.com/'$(touch SHOULD_NOT_EXIST);&";
        let token = "enr_$(touch TOKEN_MUST_NOT_EXECUTE)'\nASTER_ENROLLMENT_TOKEN\nsecret";
        let command = super::build_enrollment_command(url, token);
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .env("PATH", guard.path())
            .env("ENROLL_ARGS", &args)
            .env("ENROLL_STDIN", &stdin)
            .current_dir(guard.path())
            .status()
            .unwrap();
        assert!(status.success());
        let argv = std::fs::read(args).unwrap();
        let argv = argv
            .split(|byte| *byte == 0)
            .filter(|s| !s.is_empty())
            .map(|s| std::str::from_utf8(s).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            argv,
            [
                "node",
                "enroll",
                "--master-url",
                url,
                "--token-stdin",
                "--yes"
            ]
        );
        let mut input = String::new();
        std::fs::File::open(stdin)
            .unwrap()
            .read_to_string(&mut input)
            .unwrap();
        assert_eq!(input, format!("{token}\n"));
        assert!(!guard.path().join("SHOULD_NOT_EXIST").exists());
        assert!(!guard.path().join("TOKEN_MUST_NOT_EXECUTE").exists());
    }

    #[test]
    fn enrollment_bootstrap_defaults_legacy_payloads_to_reverse_tunnel() {
        let bootstrap: RemoteEnrollmentBootstrap = serde_json::from_value(serde_json::json!({
            "remote_node_id": 1,
            "remote_node_name": "legacy-node",
            "master_url": "https://master.example.com",
            "access_key": "access",
            "secret_key": "secret",
            "is_enabled": true,
            "ack_token": "ack"
        }))
        .expect("legacy enrollment payload should remain readable");

        assert_eq!(
            bootstrap.resolved_transport,
            aster_drive_model::types::ResolvedRemoteTransport::ReverseTunnel
        );
        assert_eq!(bootstrap.desired_revision, 1);
    }
}
