use crate::entities::resource_lock;
use crate::errors::{AsterError, Result};
use crate::types::StoredLockOwnerInfo;

use super::models::ResourceLockOwnerInfo;

pub(crate) fn serialize_resource_lock_owner_info(
    owner_info: Option<&ResourceLockOwnerInfo>,
) -> Result<Option<StoredLockOwnerInfo>> {
    let Some(owner_info) = owner_info else {
        return Ok(None);
    };

    let raw = serde_json::to_string(owner_info).map_err(|error| {
        AsterError::internal_error(format!("serialize resource lock owner payload: {error}"))
    })?;

    Ok(Some(StoredLockOwnerInfo(raw)))
}

pub(crate) fn deserialize_resource_lock_owner_info(
    lock: &resource_lock::Model,
) -> Result<Option<ResourceLockOwnerInfo>> {
    let Some(raw) = lock.owner_info.as_ref() else {
        return Ok(None);
    };
    serde_json::from_str(raw.as_ref())
        .map(Some)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "deserialize resource lock owner payload for lock #{}: {error}",
                lock.id
            ))
        })
}
