//! WebDAV 子模块：`metadata`。

use std::time::SystemTime;

use crate::entities::{file, file_blob, folder};
use crate::types::EntityType;
use crate::webdav::dav::{DavMetaData, FsResult};

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
    property_entity: Option<(EntityType, i64)>,
}

impl AsterDavMeta {
    pub fn root() -> Self {
        Self {
            is_dir: true,
            len: 0,
            modified: SystemTime::UNIX_EPOCH,
            created: SystemTime::UNIX_EPOCH,
            etag: None,
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
            property_entity: Some((EntityType::Folder, folder.id)),
        }
    }

    pub fn from_file(file: &file::Model, blob: &file_blob::Model) -> Self {
        Self {
            is_dir: false,
            len: u64::try_from(blob.size).unwrap_or_default(),
            modified: to_system_time(file.updated_at),
            created: to_system_time(file.created_at),
            etag: Some(blob.hash.clone()),
            property_entity: Some((EntityType::File, file.id)),
        }
    }
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

    fn created(&self) -> FsResult<SystemTime> {
        Ok(self.created)
    }

    fn property_entity(&self) -> Option<(EntityType, i64)> {
        self.property_entity
    }
}
