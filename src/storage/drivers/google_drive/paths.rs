//! Physical base folders beneath the configured pool, separate from logical keys.
//!
//! Google folder names are not unique and paths are not API identities. Resolve
//! each name beneath its parent instead of concatenating strings into a file ID.
//! <https://developers.google.com/workspace/drive/api/guides/folder>
//! <https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list>
use super::client::{FileInfo, escape_query};
use super::*;

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

/// Empty means the selected root. Leading/trailing slashes are presentation;
/// dot traversal, backslashes and control characters cannot change the pool.
/// The byte/depth budgets below are product request limits, not Google quotas.
pub(crate) fn normalize_base_path(value: &str) -> Result<String> {
    let value = aster_drive_storage::field_contract::normalize_object_storage_prefix(value);
    let parts: Vec<_> = value.split('/').filter(|part| !part.is_empty()).collect();
    if value.len() > 1024
        || parts.len() > 32
        || value.contains('\\')
        || value.contains("://")
        || value.chars().any(char::is_control)
        || parts
            .iter()
            .any(|part| matches!(*part, "." | "..") || part.len() > 255 || part.trim().is_empty())
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Google Drive base path must be a safe relative folder path (at most 32 components and 1024 bytes)",
        ));
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests;

impl GoogleDriveDriver {
    /// Reads/deletes never create folders. Writes serialize resolution within
    /// this driver instance; across Primaries Google's names remain non-unique,
    /// so re-query after creation and refuse ambiguity instead of choosing one.
    /// Do not recursively delete folders on failure: they may contain another
    /// writer's objects or files the administrator placed there.
    pub(super) async fn resolve_base_folder(&self, create: bool) -> Result<Option<String>> {
        if self.base_path.is_empty() {
            return Ok(Some(self.root().to_owned()));
        }
        let _resolution = self.folder_resolution.lock().await;
        let mut parent = self.root().to_owned();
        for name in self.base_path.split('/') {
            // Search every exact name, not just folders, so a file/shortcut
            // with the requested name cannot silently produce another target.
            let query = format!(
                "name='{}' and trashed=false and '{}' in parents",
                escape_query(name),
                escape_query(&parent)
            );
            let existing = self
                .find_unique(
                    &query,
                    "Google Drive base path has duplicate names; select an unambiguous folder",
                )
                .await?;
            let folder = match existing {
                Some(folder) => folder,
                None if !create => return Ok(None),
                None => {
                    let created: FileInfo = self.response(self.auth(self.http.post(format!("{}/files", self.api))
                        .query(&[("supportsAllDrives", "true"), ("fields", "id,name,mimeType")])
                        .json(&serde_json::json!({"name":name,"mimeType":FOLDER_MIME,"parents":[parent]}))).await?).await?;
                    if created.id.is_empty() || created.mime_type.as_deref() != Some(FOLDER_MIME) {
                        return Err(transient(
                            "Google Drive returned invalid base folder metadata",
                        ));
                    }
                    // files.create does not promise create-if-absent by name.
                    // A second result or an unconfirmed indexing result cannot
                    // authorize writing into an arbitrary folder.
                    let confirmed = self.find_unique(&query, "Google Drive base folder creation raced with another writer; duplicate names require reconciliation").await?
                        .ok_or_else(|| transient("Google Drive base folder creation is not yet confirmed; retry"))?;
                    if confirmed.id != created.id {
                        return Err(transient(
                            "Google Drive base folder changed during creation; retry",
                        ));
                    }
                    confirmed
                }
            };
            if folder.mime_type.as_deref() != Some(FOLDER_MIME) {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Google Drive base path component is not a folder",
                ));
            }
            parent = folder.id;
        }
        Ok(Some(parent))
    }
}
