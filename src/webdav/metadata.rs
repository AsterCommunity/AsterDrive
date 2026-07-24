//! WebDAV 子模块：`metadata`。

use std::time::SystemTime;

use crate::entities::{file, file_blob, folder};
use aster_forge_webdav::dav::{DavMetaData, DavPropertyTarget, DavResourceKind, FsResult};

/// 将 chrono DateTimeUtc 转换为 SystemTime
fn to_system_time(dt: chrono::DateTime<chrono::Utc>) -> SystemTime {
    let secs = dt.timestamp();
    match u64::try_from(secs) {
        Ok(secs) => SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
        Err(_) => SystemTime::UNIX_EPOCH,
    }
}

#[derive(Debug, Clone)]
pub struct AsterDavMeta {
    is_dir: bool,
    len: u64,
    modified: SystemTime,
    created: SystemTime,
    etag: Option<String>,
    content_type: Option<String>,
    property_entity: Option<DavPropertyTarget>,
}

impl AsterDavMeta {
    pub fn root() -> Self {
        Self {
            is_dir: true,
            len: 0,
            modified: SystemTime::UNIX_EPOCH,
            created: SystemTime::UNIX_EPOCH,
            etag: None,
            content_type: None,
            property_entity: None,
        }
    }

    pub fn from_folder(folder: &folder::Model) -> Self {
        Self {
            is_dir: true,
            len: 0,
            modified: to_system_time(folder.updated_at),
            created: to_system_time(folder.created_at),
            etag: Some(format!("dir-{}", folder.updated_at.timestamp())),
            content_type: None,
            property_entity: Some(DavPropertyTarget::new(DavResourceKind::Folder, folder.id)),
        }
    }

    pub fn from_file(file: &file::Model, _blob: &file_blob::Model) -> Self {
        Self {
            is_dir: false,
            len: u64::try_from(file.size).unwrap_or_default(),
            modified: to_system_time(file.updated_at),
            created: to_system_time(file.created_at),
            etag: Some(file_etag(file)),
            content_type: Some(file.mime_type.clone()),
            property_entity: Some(DavPropertyTarget::new(DavResourceKind::File, file.id)),
        }
    }

    pub fn from_file_record(file: &file::Model) -> Self {
        Self {
            is_dir: false,
            len: u64::try_from(file.size).unwrap_or_default(),
            modified: to_system_time(file.updated_at),
            created: to_system_time(file.created_at),
            etag: Some(file_etag(file)),
            content_type: Some(file.mime_type.clone()),
            property_entity: Some(DavPropertyTarget::new(DavResourceKind::File, file.id)),
        }
    }
}

fn file_etag(file: &file::Model) -> String {
    // File records are updated together with blob_id, size, and updated_at on
    // content replacement/version restore, so GET/HEAD and PROPFIND can share
    // this file-state validator without loading the blob in directory listings.
    format!(
        "file-{}-{}-{}-{}",
        file.id,
        file.blob_id,
        file.size,
        file.updated_at.timestamp_millis()
    )
}

impl DavMetaData for AsterDavMeta {
    fn len(&self) -> u64 {
        self.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.modified)
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }

    fn etag(&self) -> Option<String> {
        self.etag.clone()
    }

    fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    fn created(&self) -> FsResult<SystemTime> {
        Ok(self.created)
    }

    fn property_entity(&self) -> Option<DavPropertyTarget> {
        self.property_entity
    }
}
