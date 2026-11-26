//! WOPI 服务子模块：`types`。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::ops::audit::AuditRequestInfo;
use crate::services::preview::apps;

/// Maximum byte length accepted for the `X-WOPI-Lock` / `X-WOPI-OldLock` headers.
pub(crate) const MAX_WOPI_LOCK_LEN: usize = 1024;
/// Maximum byte length accepted for the `PUT_USER_INFO` body / stored user info.
pub(crate) const MAX_WOPI_USER_INFO_LEN: usize = 1024;
/// Upper bound reported to WOPI clients via `CheckFileInfo` (`FileNameMaxLength`).
pub(crate) const WOPI_FILE_NAME_MAX_LEN: i32 = 255;

/// Launch-session payload returned to the frontend when it opens a file in a
/// WOPI app.
///
/// The frontend uses these fields to post the browser to the app's action URL
/// together with the access token.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WopiLaunchSession {
    /// Opaque access token; also stored server-side as a session row so it
    /// can be revoked.
    pub access_token: String,
    /// WOPI access token expiry time as a Unix timestamp in milliseconds.
    pub access_token_ttl: i64,
    /// Fully resolved WOPI action URL (including `WOPISrc`) the app is opened at.
    pub action_url: String,
    /// App-provided form fields appended to the WOPI post target.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form_fields: BTreeMap<String, String>,
    /// How the frontend should present the app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<apps::PreviewOpenMode>,
}

/// `CheckFileInfo` response body, serialized with WOPI's PascalCase keys.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WopiCheckFileInfo {
    /// File name without any path components.
    pub base_file_name: String,
    /// Maximum file name length accepted by rename/PUT_RELATIVE, in characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name_max_length: Option<i32>,
    /// Unambiguous owner identifier (`team:{id}` or the owner's user ID).
    pub owner_id: String,
    /// File size in bytes.
    pub size: i64,
    /// Acting user's ID, distinct from `owner_id`.
    pub user_id: String,
    /// Always `false` here: PUT_RELATIVE is not restricted beyond the normal
    /// scope checks.
    pub user_can_not_write_relative: bool,
    /// Whether the current user may rename the file via `RenameFile`.
    pub user_can_rename: bool,
    /// Previously stored `PUT_USER_INFO` string echoed back to the app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_info: Option<String>,
    /// Whether the current user may write the file contents.
    pub user_can_write: bool,
    /// Whether the file is read-only for everyone.
    pub read_only: bool,
    /// Whether the `GET_LOCK` action is supported.
    pub supports_get_lock: bool,
    /// Whether the lock/unlock/refresh-lock actions are supported.
    pub supports_locks: bool,
    /// Whether lock strings may exceed the legacy 256-character limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_extended_lock_length: Option<bool>,
    /// Whether the `RENAME_FILE` action is supported.
    pub supports_rename: bool,
    /// Whether `PUT_USER_INFO` is supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_user_info: Option<bool>,
    /// Whether `PutFile` content updates are supported.
    pub supports_update: bool,
    /// File version string, also used as the `X-WOPI-ItemVersion` value.
    pub version: String,
}

/// Lock conflict detail surfaced to WOPI clients through the
/// `X-WOPI-Lock` / `X-WOPI-LockFailureReason` response headers.
#[derive(Debug, Clone)]
pub struct WopiConflict {
    /// Current lock string, or `Some("")` when unknown/not applicable.
    pub current_lock: Option<String>,
    /// Human-readable failure reason for `X-WOPI-LockFailureReason`.
    pub reason: String,
}

/// Successful `PUT_RELATIVE` response describing the newly stored file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WopiPutRelativeResponse {
    /// Final name of the created (or overwritten) file.
    pub name: String,
    /// WOPI URL of the new file with the fresh access token appended, so the
    /// app can immediately operate on it.
    pub url: String,
}

/// Successful `RENAME_FILE` response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WopiRenameFileResponse {
    /// Name the app should display afterwards; for files with an extension
    /// this is the stem only, per the WOPI convention.
    pub name: String,
}

/// `PUT_RELATIVE` conflict detail, including the optional suggested target.
#[derive(Debug, Clone)]
pub struct WopiPutRelativeConflict {
    /// Current lock string of the conflicting target, if known.
    pub current_lock: Option<String>,
    /// Human-readable failure reason for `X-WOPI-LockFailureReason`.
    pub reason: String,
    /// UTF-7 encoded available name suggested to the client, surfaced via
    /// `X-WOPI-ValidRelativeTarget`.
    pub valid_target: Option<String>,
}

/// Outcome of the `PutFile` content update.
#[derive(Debug, Clone)]
pub enum WopiPutFileResult {
    /// Content stored; `item_version` feeds the `X-WOPI-ItemVersion` header.
    Success {
        /// Item version to return in the `X-WOPI-ItemVersion` header.
        item_version: String,
    },
    /// Lock conflict, mapped to a 409 response.
    Conflict(WopiConflict),
}

/// Outcome of the `PUT_RELATIVE` operation.
#[derive(Debug, Clone)]
pub enum WopiPutRelativeResult {
    /// File stored or overwritten successfully.
    Success(WopiPutRelativeResponse),
    /// Target name or lock conflict, mapped to a 409 response.
    Conflict(WopiPutRelativeConflict),
}

/// Outcome of the `GET_LOCK` action.
#[derive(Debug, Clone)]
pub enum WopiGetLockResult {
    /// `current_lock` is the active WOPI lock string, empty when unlocked.
    Success {
        /// Active WOPI lock string, empty when the resource is unlocked.
        current_lock: String,
    },
    /// Lock conflict, mapped to a 409 response.
    Conflict(WopiConflict),
}

/// Outcome of the `LOCK` / `UNLOCK` / `REFRESH_LOCK` / unlock-and-relock actions.
#[derive(Debug, Clone)]
pub enum WopiLockOperationResult {
    /// Operation applied; mapped to a plain 200 response.
    Success,
    /// Lock mismatch or foreign lock, mapped to a 409 response.
    Conflict(WopiConflict),
}

/// Outcome of the `RENAME_FILE` action.
#[derive(Debug, Clone)]
pub enum WopiRenameFileResult {
    /// Rename applied.
    Success(WopiRenameFileResponse),
    /// Lock conflict, mapped to a 409 response.
    Conflict(WopiConflict),
    /// Requested name rejected; `reason` is surfaced via
    /// `X-WOPI-InvalidFileNameError` on a 400 response.
    InvalidName {
        /// Human-readable rejection reason sent to the WOPI client.
        reason: String,
    },
}

/// One WOPI application group parsed out of a discovery document, ready to be
/// imported as a preview app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredWopiApp {
    /// Preferred discovery action name (e.g. `edit`, `view`) for this app.
    pub action: String,
    /// File extensions the app can open, in discovery order.
    pub extensions: Vec<String>,
    /// App icon URL advertised by discovery, if any.
    pub icon_url: Option<String>,
    /// Slugified app name used to derive a unique app key suffix.
    pub key_suffix: String,
    /// Display name from discovery; falls back to `WOPI`.
    pub label: String,
}

/// Per-request provenance snapshot used for source checks and proof validation.
#[derive(Debug, Clone, Default)]
pub struct WopiRequestSource<'a> {
    /// `Origin` header value, checked against the app's trusted origins.
    pub origin: Option<&'a str>,
    /// `Referer` header value, checked when `Origin` is absent.
    pub referer: Option<&'a str>,
    /// `X-WOPI-Proof` header for Microsoft proof-key validation.
    pub proof: Option<&'a str>,
    /// `X-WOPI-ProofOld` header accepted during key rotation windows.
    pub proof_old: Option<&'a str>,
    /// `X-WOPI-TimeStamp` header bound into the proof signature.
    pub timestamp: Option<&'a str>,
    /// Absolute public request URL reconstructed from config, required as the
    /// proof signature input.
    pub public_url: Option<String>,
    /// Public site origin resolved for this request, used to build `WOPISrc`.
    pub public_origin: Option<String>,
}

/// Decoded inputs for the `PutFile` (content upload) endpoint.
pub struct WopiPutFileRequest<'a> {
    /// Target file ID taken from the route path.
    pub file_id: i64,
    /// WOPI access token from header or query string.
    pub access_token: &'a str,
    /// Raw request body streamed from the WOPI client.
    pub payload: &'a mut actix_web::web::Payload,
    /// `Content-Length` header value in bytes, when present.
    pub content_length: Option<i64>,
    /// `X-WOPI-Lock` header; required for existing files.
    pub requested_lock: Option<&'a str>,
    /// Audit context derived from the HTTP request.
    pub audit_info: &'a AuditRequestInfo,
    /// Origin/proof provenance for request validation.
    pub request_source: WopiRequestSource<'a>,
}

/// Decoded inputs for the `PUT_RELATIVE` endpoint.
pub struct WopiPutRelativeRequest<'a> {
    /// Source file ID taken from the route path; its folder receives the new
    /// file and its extension guides SuggestedTarget naming.
    pub file_id: i64,
    /// WOPI access token from header or query string.
    pub access_token: &'a str,
    /// Raw request body streamed from the WOPI client.
    pub payload: &'a mut actix_web::web::Payload,
    /// `X-WOPI-SuggestedTarget` header (UTF-7 encoded); mutually exclusive
    /// with `relative_target`.
    pub suggested_target: Option<&'a str>,
    /// `X-WOPI-RelativeTarget` header (UTF-7 encoded); mutually exclusive
    /// with `suggested_target`.
    pub relative_target: Option<&'a str>,
    /// `X-WOPI-OverwriteRelativeTarget` header controlling whether an existing
    /// relative target may be overwritten.
    pub overwrite_relative_target: Option<&'a str>,
    /// `X-WOPI-Size` header declaring the body size in bytes.
    pub size_header: Option<&'a str>,
    /// `Content-Length` header value in bytes, used when `size_header` is absent.
    pub content_length: Option<i64>,
    /// Audit context derived from the HTTP request.
    pub audit_info: &'a AuditRequestInfo,
    /// Origin/proof provenance for request validation.
    pub request_source: WopiRequestSource<'a>,
}

/// WOPI-specific owner payload deserialized from a resource lock row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WopiLockPayload {
    /// Literal `"wopi"` discriminator.
    pub(crate) kind: String,
    /// Preview app key that created the lock, kept for audit/debugging.
    pub(crate) app_key: String,
    /// Opaque lock ID from the `X-WOPI-Lock` header.
    pub(crate) lock: String,
}
