//! WebDAV 下载审计上下文适配。

use crate::entities::file;
use crate::runtime::SharedRuntimeState;
use crate::services::{
    files::file as file_ops, ops::audit::AuditContext, workspace::storage::WorkspaceStorageScope,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WebdavDownloadRequestKind {
    Full,
    Ranged,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WebdavDownloadAuditIdentity {
    pub(crate) account_id: Option<i64>,
    pub(crate) scope: WorkspaceStorageScope,
    pub(crate) root_folder_id: Option<i64>,
}

pub(crate) async fn record_download<S>(
    state: &S,
    audit_context: &AuditContext,
    identity: WebdavDownloadAuditIdentity,
    file: &file::Model,
    request_kind: WebdavDownloadRequestKind,
) where
    S: SharedRuntimeState,
{
    file_ops::record_webdav_download(
        state,
        file_ops::WebdavDownloadAuditInput {
            audit_context,
            account_id: identity.account_id,
            scope: identity.scope,
            root_folder_id: identity.root_folder_id,
            file,
            request_kind: match request_kind {
                WebdavDownloadRequestKind::Full => file_ops::WebdavDownloadRequestKind::Full,
                WebdavDownloadRequestKind::Ranged => file_ops::WebdavDownloadRequestKind::Ranged,
            },
        },
    )
    .await;
}
