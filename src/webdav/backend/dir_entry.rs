//! WebDAV 子模块：`dir_entry`。

use crate::webdav::backend::metadata::AsterDavMeta;
use aster_drive_model::entities::{file, file_revision, folder};
use aster_forge_webdav::{DavDirectoryEntry, DavVersioningState};

#[derive(Debug)]
pub struct AsterDavDirEntry {
    name: Vec<u8>,
    stable_key: Vec<u8>,
    metadata: AsterDavMeta,
    versioning: DavVersioningState,
    revision: Option<file_revision::Model>,
}

impl AsterDavDirEntry {
    pub fn from_folder(folder: &folder::Model) -> Self {
        Self {
            name: folder.name.as_bytes().to_vec(),
            stable_key: stable_key(0, folder.id),
            metadata: AsterDavMeta::from_folder(folder),
            versioning: DavVersioningState::Unsupported,
            revision: None,
        }
    }

    pub fn from_file_record(
        file: &file::Model,
        revision: file_revision::Model,
        deltav_controlled: bool,
    ) -> Self {
        Self {
            name: file.name.as_bytes().to_vec(),
            stable_key: stable_key(1, file.id),
            metadata: AsterDavMeta::from_file(file, revision.etag.clone()),
            versioning: if deltav_controlled {
                DavVersioningState::CheckedIn
            } else {
                DavVersioningState::Versionable
            },
            revision: Some(revision),
        }
    }

    pub(crate) const fn versioning_state(&self) -> DavVersioningState {
        self.versioning
    }

    pub(crate) fn current_revision(&self) -> Option<&file_revision::Model> {
        self.revision.as_ref()
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
