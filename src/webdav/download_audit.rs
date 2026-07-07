//! WebDAV 下载审计合并。

use crate::entities::file;
use crate::runtime::SharedRuntimeState;
use crate::services::{
    audit_service::{self, AuditContext, AuditEntityType},
    files::file as file_ops,
    workspace::storage::WorkspaceStorageScope,
};
use crate::utils::hash;

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct WebdavDownloadAuditIdentity {
    pub(crate) account_id: Option<i64>,
    pub(crate) scope: WorkspaceStorageScope,
    pub(crate) root_folder_id: Option<i64>,
}

pub(crate) async fn record_download<S>(
    state: &S,
    audit_ctx: &AuditContext,
    identity: WebdavDownloadAuditIdentity,
    file: &file::Model,
    request_kind: WebdavDownloadRequestKind,
) where
    S: SharedRuntimeState,
{
    if !audit_service::should_record(state, audit_service::AuditAction::FileDownload) {
        return;
    }

    if !reserve_download_audit_slot(state, audit_ctx, identity, file.id, request_kind).await {
        return;
    }

    let details = file_ops::audit_location_details_for_model(state, identity.scope, file).await;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::FileDownload,
        AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
    )
    .await;
}

async fn reserve_download_audit_slot<S>(
    state: &S,
    audit_ctx: &AuditContext,
    identity: WebdavDownloadAuditIdentity,
    file_id: i64,
    request_kind: WebdavDownloadRequestKind,
) -> bool
where
    S: SharedRuntimeState,
{
    let ttl_secs = coalesce_window_secs(state);
    if ttl_secs == 0 {
        return true;
    }

    let key = download_audit_cache_key(audit_ctx, identity, file_id, request_kind);
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
    audit_ctx: &AuditContext,
    identity: WebdavDownloadAuditIdentity,
    file_id: i64,
    request_kind: WebdavDownloadRequestKind,
) -> String {
    format!(
        "{DOWNLOAD_AUDIT_CACHE_PREFIX}{}:{}:{}:{}:{}",
        download_audit_principal(identity),
        root_folder_component(identity.root_folder_id),
        file_id,
        request_kind.as_str(),
        request_fingerprint(audit_ctx)
    )
}

fn download_audit_principal(identity: WebdavDownloadAuditIdentity) -> String {
    match identity.account_id {
        Some(account_id) => format!("account:{account_id}"),
        None => match identity.scope {
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

fn request_fingerprint(audit_ctx: &AuditContext) -> String {
    let raw = format!(
        "{}\n{}",
        audit_ctx.ip_address.as_deref().unwrap_or_default(),
        audit_ctx.user_agent.as_deref().unwrap_or_default()
    );
    hash::sha256_hex(raw.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        WebdavDownloadAuditIdentity, WebdavDownloadRequestKind, download_audit_cache_key,
        request_fingerprint,
    };
    use crate::services::{
        audit_service::AuditContext, workspace::storage::WorkspaceStorageScope,
    };

    fn audit_ctx() -> AuditContext {
        AuditContext {
            user_id: 7,
            ip_address: Some("192.0.2.10".to_string()),
            user_agent: Some("range-client/1.0".to_string()),
        }
    }

    #[test]
    fn cache_key_uses_webdav_account_when_available() {
        let identity = WebdavDownloadAuditIdentity {
            account_id: Some(42),
            scope: WorkspaceStorageScope::Personal { user_id: 7 },
            root_folder_id: None,
        };

        let key = download_audit_cache_key(
            &audit_ctx(),
            identity,
            99,
            WebdavDownloadRequestKind::Ranged,
        );

        assert!(key.contains("account:42"));
        assert!(key.contains(":99:ranged:"));
        assert!(!key.contains("192.0.2.10"));
        assert!(!key.contains("range-client"));
    }

    #[test]
    fn cache_key_separates_full_and_ranged_reads() {
        let identity = WebdavDownloadAuditIdentity {
            account_id: Some(42),
            scope: WorkspaceStorageScope::Personal { user_id: 7 },
            root_folder_id: None,
        };

        let full =
            download_audit_cache_key(&audit_ctx(), identity, 99, WebdavDownloadRequestKind::Full);
        let ranged = download_audit_cache_key(
            &audit_ctx(),
            identity,
            99,
            WebdavDownloadRequestKind::Ranged,
        );

        assert_ne!(full, ranged);
    }

    #[test]
    fn request_fingerprint_hashes_request_metadata() {
        let fingerprint = request_fingerprint(&audit_ctx());

        assert_eq!(fingerprint.len(), 64);
        assert!(!fingerprint.contains("192.0.2.10"));
    }
}
