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
    pub access_id: String,
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
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct AccessPayload {
    pub scope_kind: AdminScopeKind,
    pub target_id: i64,
    pub reason: AdminAccessReason,
    pub remark: Option<String>,
    pub download: Option<DownloadTarget>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct DownloadTarget {
    pub file_id: i64,
    pub revision_id: i64,
}
