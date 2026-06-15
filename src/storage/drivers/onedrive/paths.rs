use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::errors::{AsterError, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::object_key;

const GRAPH_PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

pub fn join_base_path(base_path: &str, path: &str) -> Result<String> {
    let sanitized = sanitize_graph_relative_path(path)?;
    if sanitized.is_empty() {
        sanitize_graph_relative_path(base_path)
    } else {
        sanitize_graph_relative_path(&object_key::join_key_prefix(base_path, &sanitized))
    }
}

pub fn normalize_graph_relative_path(path: &str) -> Result<String> {
    sanitize_graph_relative_path(path)
}

pub fn graph_drive_item_path(
    drive_id: &str,
    root_item_id: &str,
    relative_path: &str,
) -> Result<String> {
    let drive_id = required_graph_segment(drive_id, "drive_id")?;
    let root_item_id = required_graph_segment(root_item_id, "root_item_id")?;
    let relative_path = sanitize_graph_relative_path(relative_path)?;
    if relative_path.is_empty() {
        return Ok(format!("/drives/{drive_id}/items/{root_item_id}"));
    }
    Ok(format!(
        "/drives/{drive_id}/items/{root_item_id}:/{encoded}",
        encoded = encode_graph_path(&relative_path)
    ))
}

pub fn graph_drive_item_content_path(
    drive_id: &str,
    root_item_id: &str,
    relative_path: &str,
) -> Result<String> {
    let item_path = graph_drive_item_path(drive_id, root_item_id, relative_path)?;
    if sanitize_graph_relative_path(relative_path)?.is_empty() {
        Ok(format!("{item_path}/content"))
    } else {
        Ok(format!("{item_path}:/content"))
    }
}

fn required_graph_segment(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("OneDrive {field} cannot be empty"),
        ));
    }
    Ok(utf8_percent_encode(value, GRAPH_PATH_SEGMENT_ENCODE_SET).to_string())
}

fn sanitize_graph_relative_path(path: &str) -> Result<String> {
    let path = path.trim().trim_matches('/');
    if path.is_empty() {
        return Ok(String::new());
    }
    if path.contains('\\') {
        return Err(AsterError::storage_driver_error(
            "invalid OneDrive storage path: backslashes are not allowed",
        ));
    }
    let mut parts = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(AsterError::storage_driver_error(
                "invalid OneDrive storage path: parent traversal is not allowed",
            ));
        }
        parts.push(segment);
    }
    Ok(parts.join("/"))
}

fn encode_graph_path(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, GRAPH_PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_path_uses_root_item_for_empty_relative_path() {
        assert_eq!(
            graph_drive_item_path("drive", "root", "").unwrap(),
            "/drives/drive/items/root"
        );
    }

    #[test]
    fn graph_path_encodes_segments_without_encoding_slashes() {
        assert_eq!(
            graph_drive_item_path("drive", "root", "folder/a b#.txt").unwrap(),
            "/drives/drive/items/root:/folder/a%20b%23.txt"
        );
    }

    #[test]
    fn graph_content_path_uses_trailing_colon_for_relative_path() {
        assert_eq!(
            graph_drive_item_content_path("drive", "root", "folder/a.txt").unwrap(),
            "/drives/drive/items/root:/folder/a.txt:/content"
        );
        assert_eq!(
            graph_drive_item_content_path("drive", "root", "").unwrap(),
            "/drives/drive/items/root/content"
        );
    }

    #[test]
    fn join_base_path_keeps_internal_blob_key_under_configured_root() {
        assert_eq!(
            join_base_path("aster/root", "blobs/aa").unwrap(),
            "aster/root/blobs/aa"
        );
        assert_eq!(join_base_path("aster/root", "").unwrap(), "aster/root");
    }

    #[test]
    fn sanitize_rejects_parent_traversal() {
        assert!(normalize_graph_relative_path("../secret").is_err());
        assert!(normalize_graph_relative_path("folder/../../secret").is_err());
    }
}
