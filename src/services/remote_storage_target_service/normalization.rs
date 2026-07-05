use crate::entities::remote_storage_target;
use crate::errors::Result;
use crate::storage::field_contract::{
    normalize_required_storage_field, preserve_secret_when_omitted,
};
use crate::storage::remote_protocol::{
    RemoteCreateLocalStorageTargetRequest, RemoteCreateS3StorageTargetRequest,
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use crate::types::DriverType;

use super::driver::{RemoteStorageTargetDriverFields, normalize_driver_fields};

pub(in crate::services::remote_storage_target_service) struct NormalizedStorageTargetInput {
    pub name: String,
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub is_default: Option<bool>,
}

struct StorageTargetFields {
    name: String,
    driver_type: DriverType,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
    is_default: Option<bool>,
}

pub(in crate::services::remote_storage_target_service) fn normalize_create_input(
    input: RemoteCreateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    match input {
        RemoteCreateStorageTargetRequest::Local(RemoteCreateLocalStorageTargetRequest {
            name,
            base_path,
            is_default,
        }) => normalize_target_fields(StorageTargetFields {
            name: normalize_required_storage_field("name", &name)?,
            driver_type: DriverType::Local,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path,
            is_default: Some(is_default),
        }),
        RemoteCreateStorageTargetRequest::S3(RemoteCreateS3StorageTargetRequest {
            name,
            endpoint,
            bucket,
            access_key,
            secret_key,
            base_path,
            is_default,
        }) => normalize_target_fields(StorageTargetFields {
            name: normalize_required_storage_field("name", &name)?,
            driver_type: DriverType::S3,
            endpoint,
            bucket,
            access_key,
            secret_key,
            base_path,
            is_default: Some(is_default),
        }),
    }
}

pub(in crate::services::remote_storage_target_service) fn normalize_update_input(
    existing: remote_storage_target::Model,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<NormalizedStorageTargetInput> {
    let driver_type = input.driver_type.unwrap_or(existing.driver_type);
    let same_driver_type = driver_type == existing.driver_type;
    let access_key = if same_driver_type {
        preserve_secret_when_omitted("access_key", &existing.access_key, input.access_key)?
    } else {
        input.access_key.unwrap_or_default()
    };
    let secret_key = if same_driver_type {
        preserve_secret_when_omitted("secret_key", &existing.secret_key, input.secret_key)?
    } else {
        input.secret_key.unwrap_or_default()
    };
    normalize_target_fields(StorageTargetFields {
        name: input
            .name
            .as_deref()
            .map(|value| normalize_required_storage_field("name", value))
            .transpose()?
            .unwrap_or(existing.name),
        driver_type,
        endpoint: input.endpoint.unwrap_or_else(|| {
            if same_driver_type {
                existing.endpoint.clone()
            } else {
                String::new()
            }
        }),
        bucket: input.bucket.unwrap_or_else(|| {
            if same_driver_type {
                existing.bucket.clone()
            } else {
                String::new()
            }
        }),
        access_key,
        secret_key,
        base_path: input.base_path.unwrap_or_else(|| {
            if same_driver_type {
                existing.base_path.clone()
            } else {
                ".".to_string()
            }
        }),
        is_default: input.is_default,
    })
}

pub(in crate::services::remote_storage_target_service) fn new_target_key() -> String {
    format!("rst_{}", crate::utils::id::new_short_token())
}

fn normalize_target_fields(fields: StorageTargetFields) -> Result<NormalizedStorageTargetInput> {
    let StorageTargetFields {
        name,
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        is_default,
    } = fields;

    let normalized = normalize_driver_fields(RemoteStorageTargetDriverFields {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
    })?;

    Ok(NormalizedStorageTargetInput {
        name,
        driver_type: normalized.driver_type,
        endpoint: normalized.endpoint,
        bucket: normalized.bucket,
        access_key: normalized.access_key,
        secret_key: normalized.secret_key,
        base_path: normalized.base_path,
        is_default,
    })
}
