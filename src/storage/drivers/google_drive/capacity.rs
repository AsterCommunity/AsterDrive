//! Account/pool quota observation, not a reservation or a folder-size estimate.
//!
//! `about.storageQuota.usage` covers all services (including Gmail/Photos), and
//! can represent organization pooled storage. Subtracting usageInDrive instead
//! would overstate space. Google omits limit for unlimited accounts. Shared-drive
//! per-drive/owner limits are not exposed by this endpoint, so do not attribute
//! the authenticated user's personal quota to a possibly foreign shared drive.
//! <https://developers.google.com/workspace/drive/api/reference/rest/v3/about>
use super::*;
use aster_drive_storage::{StorageCapacityInfo, StorageCapacityStatus};
use chrono::Utc;

#[derive(Deserialize)]
struct About {
    #[serde(rename = "storageQuota")]
    storage_quota: Option<Quota>,
}
#[derive(Deserialize)]
struct Quota {
    limit: Option<String>,
    usage: Option<String>,
}
fn bytes(value: Option<String>) -> Result<Option<i64>> {
    value
        .map(|value| {
            value
                .parse::<i64>()
                .ok()
                .filter(|value| *value >= 0)
                .ok_or_else(|| transient("Google Drive returned invalid storage quota"))
        })
        .transpose()
}
impl GoogleDriveDriver {
    pub(super) async fn observe_capacity(&self) -> Result<StorageCapacityInfo> {
        let mut info = StorageCapacityInfo {
            status: StorageCapacityStatus::Unsupported,
            total_bytes: None,
            available_bytes: None,
            used_bytes: None,
            source: "google_drive_account".into(),
            observed_at: Utc::now(),
        };
        if self.shared_drive_id.is_some() {
            info.source = "google_drive_shared_drive".into();
            return Ok(info);
        }
        let about: About = self
            .response(
                self.auth(
                    self.http
                        .get(format!("{}/about", self.api))
                        .query(&[("fields", "storageQuota(limit,usage)")]),
                )
                .await?,
            )
            .await?;
        let Some(quota) = about.storage_quota else {
            info.status = StorageCapacityStatus::Unavailable;
            return Ok(info);
        };
        info.total_bytes = bytes(quota.limit)?;
        info.used_bytes = bytes(quota.usage)?;
        info.available_bytes = info
            .total_bytes
            .zip(info.used_bytes)
            .map(|(total, used)| total.saturating_sub(used).max(0));
        info.status = match (info.total_bytes, info.used_bytes) {
            (None, _) => StorageCapacityStatus::Unsupported,
            (Some(_), Some(_)) => StorageCapacityStatus::Supported,
            _ => StorageCapacityStatus::Unavailable,
        };
        Ok(info)
    }
}
