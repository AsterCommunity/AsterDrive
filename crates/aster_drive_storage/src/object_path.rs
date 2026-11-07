//! Physical object names owned by AsterDrive, independent of provider prefixes.
//!
//! A content hash identifies logical deduplication. New physical publications
//! receive private random names so a failed attempt cannot remove another writer's bytes.
//! Readers and cleanup use the persisted object path rather than reconstructing it
//! from a hash. Provider prefix joining stays in `object_key`; Forge owns OS temp roots.

use crate::error::{Result, StorageErrorKind, storage_driver_error};

const THUMBNAIL_PREFIX: &str = "_thumb";
const IMAGE_PREVIEW_PREFIX: &str = "_preview";
const BLOB_PREFIX: &str = "files";

/// Allocate an immutable object candidate owned by one publication attempt.
pub fn new_blob_path() -> String {
    format!("{BLOB_PREFIX}/{}", aster_forge_utils::id::new_uuid())
}

/// The object initially owned by one UUID upload session.
pub fn upload_path(upload_id: &str) -> Result<String> {
    let upload_id =
        uuid::Uuid::parse_str(upload_id).map_err(|_| invalid("upload id must be a UUID"))?;
    Ok(format!("{BLOB_PREFIX}/{upload_id}"))
}

/// Original-filename delivery retains the upload's UUID as its ownership boundary.
pub fn named_upload_path(upload_id: &str, filename: Option<&str>) -> Result<String> {
    let parent = upload_path(upload_id)?;
    match filename {
        Some(filename) => {
            let filename = aster_forge_validation::filename::normalize_validate_name(filename)
                .map_err(|error| invalid(error.to_string()))?;
            Ok(format!("{parent}/{filename}"))
        }
        None => Ok(parent),
    }
}

/// A local opaque blob retains its two-level directory fanout.
pub fn new_local_blob() -> (String, String) {
    let key = aster_forge_utils::id::new_short_token();
    let path = format!("{}/{}/{key}", &key[..2], &key[2..4]);
    (key, path)
}

/// Local ingest files are private siblings within the configured storage volume.
pub fn new_upload_staging_name() -> String {
    format!("{}.upload", aster_forge_utils::id::new_uuid())
}

pub fn local_upload_staging_path(name: &str) -> Result<String> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(invalid(
            "local upload staging name must be a safe storage token",
        ));
    }
    Ok(format!(".staging/{name}"))
}

/// Filesystem-like uploads stage beside their destination for atomic rename.
pub fn stream_staging_path(destination: &str, attempt_id: &str) -> String {
    match destination.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/.aster-attempt-{attempt_id}.tmp"),
        None => format!(".aster-attempt-{attempt_id}.tmp"),
    }
}

/// A relay's complete candidate remains private until digest verification succeeds.
pub fn new_relay_attempt_path(destination: &str) -> String {
    format!(
        "{destination}.aster-attempt-{}",
        aster_forge_utils::id::new_uuid()
    )
}

/// A filesystem-native copy never replaces its destination until copying finishes.
pub fn new_copy_staging_path(destination: &str) -> String {
    format!(
        "{destination}.aster-copy-{}.tmp",
        aster_forge_utils::id::new_uuid()
    )
}

/// Cross-device promotion first materializes one exclusive sibling file.
pub fn new_promotion_staging_name() -> String {
    format!(".aster-promote-{}.tmp", aster_forge_utils::id::new_uuid())
}

/// Remote multipart parts are scoped to a single product-owned upload identifier.
pub fn multipart_parts_prefix(upload_id: &str) -> Result<String> {
    if upload_id.is_empty()
        || !upload_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid("multipart upload id must be a safe storage token"));
    }
    Ok(format!("uploads/{upload_id}/parts"))
}

pub fn multipart_part_path(upload_id: &str, part_number: i32) -> Result<String> {
    if part_number <= 0 {
        return Err(invalid(format!(
            "multipart part_number must be positive, got {part_number}"
        )));
    }
    Ok(format!(
        "{}/{part_number}",
        multipart_parts_prefix(upload_id)?
    ))
}

/// Derivative namespaces remain stable for persisted cache metadata.
pub fn thumbnail_path(blob_hash: &str, processor: &str, version: &str) -> String {
    derivative_path(THUMBNAIL_PREFIX, blob_hash, processor, version)
}

pub fn image_preview_path(blob_hash: &str, processor: &str, version: &str) -> String {
    derivative_path(IMAGE_PREVIEW_PREFIX, blob_hash, processor, version)
}

fn derivative_path(namespace: &str, blob_hash: &str, processor: &str, version: &str) -> String {
    // Product-generated hashes and synthetic keys are ASCII. Character-based fanout
    // also makes old non-ASCII records diagnosable rather than panicking on a byte slice.
    let first = blob_hash.chars().take(2).collect::<String>();
    let second = blob_hash.chars().skip(2).take(2).collect::<String>();
    format!("{namespace}/{processor}/{version}/{first}/{second}/{blob_hash}.webp")
}

pub fn is_thumbnail_path(path: &str) -> bool {
    path.trim_start_matches('/')
        .strip_prefix(THUMBNAIL_PREFIX)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn is_image_preview_path(path: &str) -> bool {
    path.trim_start_matches('/')
        .strip_prefix(IMAGE_PREVIEW_PREFIX)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn invalid(message: impl Into<String>) -> crate::StorageError {
    storage_driver_error(StorageErrorKind::Precondition, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_candidates_are_unique_and_session_paths_are_canonical() {
        let first = new_blob_path();
        assert_ne!(first, new_blob_path());
        assert!(uuid::Uuid::parse_str(first.strip_prefix("files/").unwrap()).is_ok());
        let id = "4D59EDE3-774F-4B70-B43E-EFADAE8BBAF1";
        assert_eq!(
            upload_path(id).unwrap(),
            "files/4d59ede3-774f-4b70-b43e-efadae8bbaf1"
        );
        assert_eq!(
            named_upload_path(id, Some("资料.txt")).unwrap(),
            "files/4d59ede3-774f-4b70-b43e-efadae8bbaf1/资料.txt"
        );
        assert_eq!(
            named_upload_path(id, None).unwrap(),
            upload_path(id).unwrap()
        );
        for invalid in ["", "../escape", "not-a-uuid"] {
            assert!(upload_path(invalid).is_err());
        }
        for filename in ["../escape", "nested/file.txt", "CON", "file."] {
            assert!(named_upload_path(id, Some(filename)).is_err());
        }
    }

    #[test]
    fn multipart_and_staging_names_keep_their_ownership_boundaries() {
        assert_eq!(
            multipart_part_path("upload-1", 3).unwrap(),
            "uploads/upload-1/parts/3"
        );
        for id in ["", "..", "../upload", "nested/upload", "nested\\upload"] {
            assert!(multipart_parts_prefix(id).is_err());
        }
        assert!(multipart_part_path("upload-1", 0).is_err());
        assert!(multipart_part_path("upload-1", -1).is_err());
        assert_eq!(
            stream_staging_path("files/final", "attempt"),
            "files/.aster-attempt-attempt.tmp"
        );
        assert_eq!(
            stream_staging_path("final", "attempt"),
            ".aster-attempt-attempt.tmp"
        );
        let upload_name = new_upload_staging_name();
        assert_eq!(
            local_upload_staging_path(&upload_name).unwrap(),
            format!(".staging/{upload_name}")
        );
        assert!(local_upload_staging_path("../escape").is_err());
        assert!(local_upload_staging_path(".").is_err());
        assert!(local_upload_staging_path("nested/file").is_err());
        let (key, path) = new_local_blob();
        assert_eq!(path, format!("{}/{}/{key}", &key[..2], &key[2..4]));
    }

    #[test]
    fn derivatives_preserve_cache_paths_and_classify_complete_namespaces() {
        let hash = "abcdef0123456789";
        assert_eq!(
            thumbnail_path(hash, "images", "1"),
            "_thumb/images/1/ab/cd/abcdef0123456789.webp"
        );
        assert_eq!(
            image_preview_path(hash, "vips", "1-d2048"),
            "_preview/vips/1-d2048/ab/cd/abcdef0123456789.webp"
        );
        assert!(is_thumbnail_path("/_thumb/images/1/file.webp"));
        assert!(!is_thumbnail_path("_thumbnail/file.webp"));
        assert!(is_image_preview_path("_preview/images/1/file.webp"));
        assert!(!is_image_preview_path("_preview-else/file.webp"));
        assert!(thumbnail_path("猫猫", "images", "1").ends_with("猫猫.webp"));
    }
}
