//! Bounded wire operation labels, never raw paths or object keys.
use aster_drive_metrics::RemoteOperation;
pub(super) fn operation(method: &str, path_and_query: &str) -> RemoteOperation {
    if path_and_query.split('?').next().is_some_and(|path| {
        path.strip_prefix(super::INTERNAL_STORAGE_BASE_PATH) == Some("/capacity")
    }) {
        return RemoteOperation::Capacity;
    }
    match method {
        "GET" => RemoteOperation::Read,
        "HEAD" => RemoteOperation::Metadata,
        "PUT" => RemoteOperation::Write,
        "DELETE" => RemoteOperation::Delete,
        _ => RemoteOperation::Control,
    }
}
