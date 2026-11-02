use serde::Serialize;
#[derive(Debug, Serialize)]
pub(in crate::cli) struct NodeEnrollReport {
    pub(super) action: &'static str,
    pub(super) binding_id: i64,
    pub(super) binding_name: String,
    pub(super) master_url: String,
    pub(super) access_key: String,
    pub(super) config_path: String,
    pub(super) server_host: String,
    pub(super) server_port: u16,
    pub(super) readiness_check_path: String,
    pub(super) connectivity_hint: String,
}
