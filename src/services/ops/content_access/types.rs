use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Workspace scope an administrator content access grant applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminScopeKind {
    /// One user's personal workspace.
    Personal,
    /// One team workspace.
    Team,
}

/// Declared purpose of an administrator content access grant, stored with its audit evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminAccessReason {
    /// Resolving a user's support request.
    UserSupport,
    /// Recovering user data after loss or corruption.
    DataRecovery,
    /// Investigating a security incident.
    SecurityInvestigation,
    /// Any other documented operational reason.
    Other,
}

/// Request to create a time-limited administrator content access grant.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateAdminContentAccess {
    /// Workspace the grant targets.
    pub scope_kind: AdminScopeKind,
    /// User or team ID selected by [`AdminScopeKind`]; must be positive.
    pub target_id: i64,
    /// Declared purpose; required, missing reasons are rejected.
    pub reason: Option<AdminAccessReason>,
    /// Free-form note trimmed and capped at 500 characters without control characters.
    pub remark: Option<String>,
}

/// Created grant handle returned to the administrator client.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminContentAccessInfo {
    /// Opaque grant ID used to authorize subsequent content operations.
    pub id: String,
    /// Grant expiry; authorization fails once past this instant.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
}

/// Reports whether administrator content access is enabled instance-wide.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminContentAccessStatus {
    /// `true` when new grants may be created.
    pub enabled: bool,
}

/// Request to initialize an administrator download of one file or revision.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AdminDownloadInitRequest {
    /// Existing workspace grant ID; when omitted, the scope is derived from the file itself.
    pub access_id: Option<String>,
    /// Revision to download; when omitted, the file's current content is used.
    pub revision_id: Option<i64>,
}

/// Prepared administrator download pointing at a short-lived resource URL.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminDownloadInfo {
    /// Delivery handle carrying the download URL, lifecycle expiry and redirect policy.
    pub resource: crate::api::dto::files::FileResourceHandle,
    /// File the download was prepared for.
    pub file_id: i64,
    /// Revision whose content the URL resolves to.
    pub revision_id: i64,
    /// File name suggested for the attachment disposition.
    pub name: String,
    /// Logical file size in bytes.
    pub size: i64,
}

/// Workspace a grant is scoped to, stored inside the durable grant payload.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccessScope {
    /// Whether the target is a personal workspace or a team workspace.
    pub scope_kind: AdminScopeKind,
    /// User ID for [`AdminScopeKind::Personal`], team ID for [`AdminScopeKind::Team`].
    pub target_id: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceAccess {
    pub scope: AccessScope,
    pub reason: AdminAccessReason,
    pub remark: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AccessPayload {
    Workspace(WorkspaceAccess),
    Download {
        scope: AccessScope,
        file_id: i64,
        revision_id: i64,
    },
}

impl AccessPayload {
    pub fn scope(&self) -> AccessScope {
        match self {
            Self::Workspace(access) => access.scope,
            Self::Download { scope, .. } => *scope,
        }
    }
}
