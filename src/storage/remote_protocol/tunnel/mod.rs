//! Reverse tunnel protocol runtime.

use percent_encoding::percent_decode_str;

pub mod client;
pub mod server;

pub(crate) fn is_allowed_tunnel_target(path_and_query: &str) -> bool {
    let path = path_and_query
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(path_and_query);
    if path.is_empty() || !path.starts_with('/') || path.contains('#') || path.contains('\\') {
        return false;
    }

    let decoded = percent_decode_str(path).decode_utf8_lossy();
    if decoded.contains('\\')
        || decoded
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return false;
    }

    is_internal_storage_path(path) && is_internal_storage_path(decoded.as_ref())
}

fn is_internal_storage_path(path: &str) -> bool {
    path == super::INTERNAL_STORAGE_BASE_PATH
        || path.starts_with(&format!("{}/", super::INTERNAL_STORAGE_BASE_PATH))
}
