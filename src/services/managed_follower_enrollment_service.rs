//! 服务模块：`managed_follower_enrollment_service`。

use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::db::repository::{follower_enrollment_session_repo, managed_follower_repo};
use crate::entities::follower_enrollment_session;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::SharedRuntimeState;
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
    pub ack_token: String,
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
    let token = format!("enr_{}", crate::utils::id::new_short_token());
    let ack_token = format!("enr_ack_{}", crate::utils::id::new_short_token());
    let token_hash = crate::utils::hash::sha256_hex(token.as_bytes());
    let ack_token_hash = crate::utils::hash::sha256_hex(ack_token.as_bytes());
    let expires_at = Utc::now() + Duration::minutes(DEFAULT_ENROLLMENT_TTL_MINUTES);
    let created_at = Utc::now();

    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
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
        Ok(())
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

    let token_hash = crate::utils::hash::sha256_hex(trimmed.as_bytes());
    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        let enrollment = follower_enrollment_session_repo::find_by_token_hash(txn, &token_hash)
            .await?
            .ok_or_else(|| AsterError::validation_error("invalid enrollment token"))?;

        if enrollment.invalidated_at.is_some() {
            return Err(AsterError::validation_error(
                ENROLLMENT_TOKEN_REPLACED_MESSAGE,
            ));
        }
        if enrollment.acked_at.is_some() {
            return Err(AsterError::validation_error(
                ENROLLMENT_TOKEN_COMPLETED_MESSAGE,
            ));
        }
        if enrollment.expires_at <= Utc::now() {
            return Err(AsterError::validation_error(
                ENROLLMENT_TOKEN_EXPIRED_MESSAGE,
            ));
        }

        follower_enrollment_session_repo::mark_redeemed_if_needed(txn, enrollment.id).await?;

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
            ack_token: format!("enr_ack_{}", &enrollment.ack_token_hash),
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
    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
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
    format!("{ENROLL_COMMAND_BINARY} node enroll --master-url {master_url} --token {token}")
}

fn normalize_ack_token_hash(value: &str) -> String {
    if let Some(raw_hash) = value.strip_prefix("enr_ack_")
        && raw_hash.len() == 64
        && raw_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return raw_hash.to_ascii_lowercase();
    }

    crate::utils::hash::sha256_hex(value.as_bytes())
}
