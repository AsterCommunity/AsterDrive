//! WebDAV 子模块：`dir_entry`。

use crate::webdav::backend::metadata::AsterDavMeta;
use aster_drive_model::entities::{file, folder};
use aster_forge_webdav::DavDirectoryEntry;

#[derive(Debug)]
pub struct AsterDavDirEntry {
    name: Vec<u8>,
    stable_key: Vec<u8>,
    metadata: AsterDavMeta,
}

impl AsterDavDirEntry {
    pub fn from_folder(folder: &folder::Model) -> Self {
        Self {
            name: folder.name.as_bytes().to_vec(),
            stable_key: stable_key(0, folder.id),
            metadata: AsterDavMeta::from_folder(folder),
        }
    }

    pub fn from_file_record(file: &file::Model) -> Self {
        Self {
            name: file.name.as_bytes().to_vec(),
            stable_key: stable_key(1, file.id),
            metadata: AsterDavMeta::from_file_record(file),
        }
    }
}

fn stable_key(kind: u8, id: i64) -> Vec<u8> {
    let mut key = Vec::with_capacity(9);
    key.push(kind);
    key.extend_from_slice(&id.to_be_bytes());
    key
}

impl DavDirectoryEntry for AsterDavDirEntry {
    type Metadata = AsterDavMeta;

    fn name(&self) -> &[u8] {
        &self.name
    }

    fn metadata(&self) -> &Self::Metadata {
        &self.metadata
    }

    fn stable_key(&self) -> &[u8] {
        &self.stable_key
    }
}
