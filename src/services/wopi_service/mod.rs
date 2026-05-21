//! WOPI 集成入口。
//!
//! 这组模块把“WOPI 应用发现、会话 token、锁、文件读写和 PUT_RELATIVE / rename”
//! 这些 WOPI 语义拆开处理。调用方通常只看这里导出的高层操作函数。

mod discovery;
mod locks;
mod operations;
mod proof;
mod session;
mod targets;
#[cfg(test)]
mod tests;
mod types;

pub use discovery::{allowed_origins, discover_apps};
pub use locks::{get_lock, lock_file, refresh_lock, unlock_and_relock_file, unlock_file};
pub use operations::{
    WopiGetFileResult, check_file_info, get_file_contents, put_file_contents, put_relative_file,
    put_user_info, rename_file,
};
pub use session::cleanup_expired;
pub(crate) use session::{RequestOrigin, create_launch_session_in_scope};
pub use types::{
    DiscoveredWopiApp, WopiCheckFileInfo, WopiConflict, WopiGetLockResult, WopiLaunchSession,
    WopiLockOperationResult, WopiPutFileRequest, WopiPutFileResult, WopiPutRelativeConflict,
    WopiPutRelativeRequest, WopiPutRelativeResponse, WopiPutRelativeResult, WopiRenameFileResponse,
    WopiRenameFileResult, WopiRequestSource,
};
