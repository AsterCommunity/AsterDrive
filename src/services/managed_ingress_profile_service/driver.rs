use std::path::Path;
use std::sync::Arc;

use crate::entities::{managed_ingress_profile, storage_policy};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::FollowerRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::drivers::{local::LocalDriver, s3::S3Driver};
use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};

use super::paths::resolve_managed_local_path;

pub(in crate::services::managed_ingress_profile_service) fn validate_driver_from_profile<
    S: FollowerRuntimeState,
>(
    state: &S,
    profile: &managed_ingress_profile::Model,
) -> Result<()> {
    let policy = build_policy_model(state, profile)?;
    match policy.driver_type {
        DriverType::Local => {
            let base_path = Path::new(&policy.base_path);
            std::fs::create_dir_all(base_path).map_aster_err_ctx(
                &format!(
                    "create managed ingress local path '{}'",
                    base_path.display()
                ),
                AsterError::storage_driver_error,
            )
        }
        DriverType::S3 => S3Driver::validate_policy(&policy),
        DriverType::TencentCos | DriverType::Remote => {
            Err(managed_ingress_unsupported_driver_error(policy.driver_type))
        }
    }
}

pub(in crate::services::managed_ingress_profile_service) fn build_driver_from_profile<
    S: FollowerRuntimeState,
>(
    state: &S,
    profile: &managed_ingress_profile::Model,
) -> Result<Arc<dyn StorageDriver>> {
    let policy = build_policy_model(state, profile)?;
    match policy.driver_type {
        DriverType::Local => {
            let base_path = Path::new(&policy.base_path);
            std::fs::create_dir_all(base_path).map_aster_err_ctx(
                &format!(
                    "create managed ingress local path '{}'",
                    base_path.display()
                ),
                AsterError::storage_driver_error,
            )?;
            Ok(Arc::new(LocalDriver::new(&policy)?))
        }
        DriverType::S3 => Ok(Arc::new(S3Driver::new(&policy)?)),
        DriverType::TencentCos | DriverType::Remote => {
            Err(managed_ingress_unsupported_driver_error(policy.driver_type))
        }
    }
}

fn managed_ingress_unsupported_driver_error(driver_type: DriverType) -> AsterError {
    AsterError::validation_error(format!(
        "managed ingress profiles do not support the {} driver",
        match driver_type {
            DriverType::TencentCos => "tencent_cos",
            DriverType::Remote => "remote",
            other => other.as_str(),
        }
    ))
}

fn build_policy_model<S: FollowerRuntimeState>(
    state: &S,
    profile: &managed_ingress_profile::Model,
) -> Result<storage_policy::Model> {
    let base_path = match profile.driver_type {
        DriverType::Local => resolve_managed_local_path(
            &state.config().server.follower.managed_ingress_local_root,
            &profile.base_path,
        )?
        .to_string_lossy()
        .into_owned(),
        DriverType::S3 => profile.base_path.clone(),
        DriverType::TencentCos | DriverType::Remote => String::new(),
    };

    Ok(storage_policy::Model {
        id: profile.id,
        name: profile.name.clone(),
        driver_type: profile.driver_type,
        endpoint: profile.endpoint.clone(),
        bucket: profile.bucket.clone(),
        access_key: profile.access_key.clone(),
        secret_key: profile.secret_key.clone(),
        base_path,
        remote_node_id: None,
        max_file_size: profile.max_file_size,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        options: StoredStoragePolicyOptions::empty(),
        is_default: profile.is_default,
        chunk_size: 0,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    })
}
