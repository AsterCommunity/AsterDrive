//! WebDAV 子模块：`dir_entry`。

use crate::entities::{file, file_blob, folder};
use crate::webdav::dav::{DavDirEntry, DavMetaData, FsFuture};
use crate::webdav::metadata::AsterDavMeta;

#[derive(Debug)]
pub struct AsterDavDirEntry {
    name: Vec<u8>,
    metadata: AsterDavMeta,
}

impl AsterDavDirEntry {
    pub fn from_folder(folder: &folder::Model) -> Self {
        Self {
            name: folder.name.as_bytes().to_vec(),
            metadata: AsterDavMeta::from_folder(folder),
        }
    }

    pub fn from_file(file: &file::Model, blob: &file_blob::Model) -> Self {
        Self {
            name: file.name.as_bytes().to_vec(),
            metadata: AsterDavMeta::from_file(file, blob),
        }
    }

    pub fn from_file_record(file: &file::Model) -> Self {
        Self {
            name: file.name.as_bytes().to_vec(),
            metadata: AsterDavMeta::from_file_record(file),
        }
    }
}

impl DavDirEntry for AsterDavDirEntry {
    fn name(&self) -> Vec<u8> {
        self.name.clone()
    }

    fn metadata<'a>(&'a self) -> FsFuture<'a, Box<dyn DavMetaData>> {
        let meta = self.metadata.clone();
        Box::pin(async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) })
    }
}
