use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminScopeKind {
    Personal,
    Team,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminAccessReason {
    UserSupport,
    DataRecovery,
    SecurityInvestigation,
    Other,
}
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateAdminContentAccess {
    pub scope_kind: AdminScopeKind,
    pub target_id: i64,
    pub reason: Option<AdminAccessReason>,
    pub remark: Option<String>,
}
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminContentAccessInfo {
    pub id: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
}
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminContentAccessStatus {
    pub enabled: bool,
}
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(deny_unknown_fields)]
pub struct AdminDownloadInitRequest {
    pub access_id: Option<String>,
    pub revision_id: Option<i64>,
}
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminDownloadInfo {
    pub resource: crate::api::dto::files::FileResourceHandle,
    pub file_id: i64,
    pub revision_id: i64,
    pub name: String,
    pub size: i64,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccessScope {
    pub scope_kind: AdminScopeKind,
    pub target_id: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkspaceAccess {
    pub scope: AccessScope,
    pub reason: AdminAccessReason,
    pub remark: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AccessPayload {
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
