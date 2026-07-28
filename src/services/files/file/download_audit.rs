use crate::runtime::SharedRuntimeState;
use crate::services::{
    ops::audit::{self, AuditContext, AuditEntityType},
    workspace::storage::WorkspaceStorageScope,
};
use aster_drive_model::entities::file;
use aster_forge_crypto as hash;

const DOWNLOAD_AUDIT_CACHE_PREFIX: &str = "webdav_download_audit:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WebdavDownloadRequestKind {
    Full,
    Ranged,
}

impl WebdavDownloadRequestKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Ranged => "ranged",
        }
    }
}

pub(crate) struct WebdavDownloadAuditInput<'a> {
    pub(crate) audit_context: &'a AuditContext,
    pub(crate) account_id: Option<i64>,
    pub(crate) scope: WorkspaceStorageScope,
    pub(crate) root_folder_id: Option<i64>,
    pub(crate) file: &'a file::Model,
    pub(crate) request_kind: WebdavDownloadRequestKind,
}

pub(crate) async fn record_webdav_download<S>(state: &S, input: WebdavDownloadAuditInput<'_>)
where
    S: SharedRuntimeState,
{
    if !audit::should_record(state, audit::AuditAction::FileDownload) {
        return;
    }

    if !reserve_download_audit_slot(state, &input).await {
        return;
    }

    let details = super::audit_location_details_for_model(state, input.scope, input.file).await;
    audit::log_with_details(
        state,
        input.audit_context,
        audit::AuditAction::FileDownload,
        AuditEntityType::File,
        Some(input.file.id),
        Some(&input.file.name),
        || details.clone(),
    )
    .await;
}

async fn reserve_download_audit_slot<S>(state: &S, input: &WebdavDownloadAuditInput<'_>) -> bool
where
    S: SharedRuntimeState,
{
    let ttl_secs = coalesce_window_secs(state);
    if ttl_secs == 0 {
        return true;
    }

    let key = download_audit_cache_key(
        input.audit_context,
        input.account_id,
        input.scope,
        input.root_folder_id,
        input.file.id,
        input.request_kind,
    );
    state
        .cache()
        .set_bytes_if_absent(&key, Vec::new(), Some(ttl_secs))
        .await
}

fn coalesce_window_secs(state: &impl SharedRuntimeState) -> u64 {
    state.runtime_config().get_u64_or(
        crate::config::definitions::WEBDAV_DOWNLOAD_AUDIT_COALESCE_WINDOW_SECS_KEY,
        crate::config::definitions::DEFAULT_WEBDAV_DOWNLOAD_AUDIT_COALESCE_WINDOW_SECS,
    )
}

fn download_audit_cache_key(
    audit_context: &AuditContext,
    account_id: Option<i64>,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    file_id: i64,
    request_kind: WebdavDownloadRequestKind,
) -> String {
    format!(
        "{DOWNLOAD_AUDIT_CACHE_PREFIX}{}:{}:{}:{}:{}",
        download_audit_principal(account_id, scope),
        root_folder_component(root_folder_id),
        file_id,
        request_kind.as_str(),
        request_fingerprint(audit_context)
    )
}

fn download_audit_principal(account_id: Option<i64>, scope: WorkspaceStorageScope) -> String {
    match account_id {
        Some(account_id) => format!("account:{account_id}"),
        None => match scope {
            WorkspaceStorageScope::Personal { user_id } => format!("personal:{user_id}"),
            WorkspaceStorageScope::Team {
                team_id,
                actor_user_id,
            } => format!("team:{team_id}:actor:{actor_user_id}"),
        },
    }
}

fn root_folder_component(root_folder_id: Option<i64>) -> String {
    match root_folder_id {
        Some(root_folder_id) => root_folder_id.to_string(),
        None => "root".to_string(),
    }
}

fn request_fingerprint(audit_context: &AuditContext) -> String {
    let raw = format!(
        "{}\n{}",
        audit_context.ip_address.as_deref().unwrap_or_default(),
        audit_context.user_agent.as_deref().unwrap_or_default()
    );
    hash::sha256_hex(raw.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{WebdavDownloadRequestKind, download_audit_cache_key, request_fingerprint};
    use crate::services::{ops::audit::AuditContext, workspace::storage::WorkspaceStorageScope};

    fn audit_context() -> AuditContext {
        AuditContext {
            user_id: 7,
            ip_address: Some("192.0.2.10".to_string()),
            user_agent: Some("range-client/1.0".to_string()),
        }
    }

    fn cache_key(request_kind: WebdavDownloadRequestKind) -> String {
        download_audit_cache_key(
            &audit_context(),
            Some(42),
            WorkspaceStorageScope::Personal { user_id: 7 },
            None,
            99,
            request_kind,
        )
    }

    #[test]
    fn cache_key_uses_webdav_account_when_available() {
        let key = cache_key(WebdavDownloadRequestKind::Ranged);

        assert!(key.contains("account:42"));
        assert!(key.contains(":99:ranged:"));
        assert!(!key.contains("192.0.2.10"));
        assert!(!key.contains("range-client"));
    }

    #[test]
    fn cache_key_separates_full_and_ranged_reads() {
        assert_ne!(
            cache_key(WebdavDownloadRequestKind::Full),
            cache_key(WebdavDownloadRequestKind::Ranged)
        );
    }

    #[test]
    fn request_fingerprint_hashes_request_metadata() {
        let fingerprint = request_fingerprint(&audit_context());

        assert_eq!(fingerprint.len(), 64);
        assert!(!fingerprint.contains("192.0.2.10"));
    }
}
