//! WOPI 服务子模块：`types`。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::ops::audit::AuditRequestInfo;
use crate::services::preview::apps;

pub(crate) const MAX_WOPI_LOCK_LEN: usize = 1024;
pub(crate) const MAX_WOPI_USER_INFO_LEN: usize = 1024;
pub(crate) const WOPI_FILE_NAME_MAX_LEN: i32 = 255;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WopiLaunchSession {
    pub access_token: String,
    /// WOPI access token expiry time as a Unix timestamp in milliseconds.
    pub access_token_ttl: i64,
    pub action_url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form_fields: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<apps::PreviewOpenMode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WopiCheckFileInfo {
    pub base_file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name_max_length: Option<i32>,
    pub owner_id: String,
    pub size: i64,
    pub user_id: String,
    pub user_can_not_write_relative: bool,
    pub user_can_rename: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_info: Option<String>,
    pub user_can_write: bool,
    pub read_only: bool,
    pub supports_get_lock: bool,
    pub supports_locks: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_extended_lock_length: Option<bool>,
    pub supports_rename: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_user_info: Option<bool>,
    pub supports_update: bool,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct WopiConflict {
    pub current_lock: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WopiPutRelativeResponse {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WopiRenameFileResponse {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct WopiPutRelativeConflict {
    pub current_lock: Option<String>,
    pub reason: String,
    pub valid_target: Option<String>,
}

#[derive(Debug, Clone)]
pub enum WopiPutFileResult {
    Success { item_version: String },
    Conflict(WopiConflict),
}

#[derive(Debug, Clone)]
pub enum WopiPutRelativeResult {
    Success(WopiPutRelativeResponse),
    Conflict(WopiPutRelativeConflict),
}

#[derive(Debug, Clone)]
pub enum WopiGetLockResult {
    Success { current_lock: String },
    Conflict(WopiConflict),
}

#[derive(Debug, Clone)]
pub enum WopiLockOperationResult {
    Success,
    Conflict(WopiConflict),
}

#[derive(Debug, Clone)]
pub enum WopiRenameFileResult {
    Success(WopiRenameFileResponse),
    Conflict(WopiConflict),
    InvalidName { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredWopiApp {
    pub action: String,
    pub extensions: Vec<String>,
    pub icon_url: Option<String>,
    pub key_suffix: String,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct WopiRequestSource<'a> {
    pub origin: Option<&'a str>,
    pub referer: Option<&'a str>,
    pub proof: Option<&'a str>,
    pub proof_old: Option<&'a str>,
    pub timestamp: Option<&'a str>,
    pub public_url: Option<String>,
    pub public_origin: Option<String>,
}

pub struct WopiPutFileRequest<'a> {
    pub file_id: i64,
    pub access_token: &'a str,
    pub payload: &'a mut actix_web::web::Payload,
    pub content_length: Option<i64>,
    pub requested_lock: Option<&'a str>,
    pub audit_info: &'a AuditRequestInfo,
    pub request_source: WopiRequestSource<'a>,
}

pub struct WopiPutRelativeRequest<'a> {
    pub file_id: i64,
    pub access_token: &'a str,
    pub payload: &'a mut actix_web::web::Payload,
    pub suggested_target: Option<&'a str>,
    pub relative_target: Option<&'a str>,
    pub overwrite_relative_target: Option<&'a str>,
    pub size_header: Option<&'a str>,
    pub content_length: Option<i64>,
    pub audit_info: &'a AuditRequestInfo,
    pub request_source: WopiRequestSource<'a>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WopiLockPayload {
    pub(crate) kind: String,
    pub(crate) app_key: String,
    pub(crate) lock: String,
}
