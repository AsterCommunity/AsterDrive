use crate::entities::managed_ingress_profile;
use crate::errors::{AsterError, Result};
use crate::storage::remote_protocol::{
    RemoteCreateIngressProfileRequest, RemoteCreateLocalIngressProfileRequest,
    RemoteCreateS3IngressProfileRequest, RemoteUpdateIngressProfileRequest,
};
use crate::types::DriverType;

use super::driver::{ManagedIngressDriverFields, normalize_driver_fields};

pub(in crate::services::managed_ingress_profile_service) struct NormalizedIngressProfileInput {
    pub name: String,
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub max_file_size: i64,
    pub is_default: Option<bool>,
}

struct IngressProfileFields {
    name: String,
    driver_type: DriverType,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
    max_file_size: i64,
    is_default: Option<bool>,
}

pub(in crate::services::managed_ingress_profile_service) fn normalize_create_input(
    input: RemoteCreateIngressProfileRequest,
) -> Result<NormalizedIngressProfileInput> {
    match input {
        RemoteCreateIngressProfileRequest::Local(RemoteCreateLocalIngressProfileRequest {
            name,
            base_path,
            max_file_size,
            is_default,
        }) => normalize_profile_fields(IngressProfileFields {
            name: normalize_non_blank("name", &name)?,
            driver_type: DriverType::Local,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path,
            max_file_size,
            is_default: Some(is_default),
        }),
        RemoteCreateIngressProfileRequest::S3(RemoteCreateS3IngressProfileRequest {
            name,
            endpoint,
            bucket,
            access_key,
            secret_key,
            base_path,
            max_file_size,
            is_default,
        }) => normalize_profile_fields(IngressProfileFields {
            name: normalize_non_blank("name", &name)?,
            driver_type: DriverType::S3,
            endpoint,
            bucket,
            access_key,
            secret_key,
            base_path,
            max_file_size,
            is_default: Some(is_default),
        }),
    }
}

pub(in crate::services::managed_ingress_profile_service) fn normalize_update_input(
    existing: managed_ingress_profile::Model,
    input: RemoteUpdateIngressProfileRequest,
) -> Result<NormalizedIngressProfileInput> {
    let driver_type = input.driver_type.unwrap_or(existing.driver_type);
    let same_driver_type = driver_type == existing.driver_type;
    normalize_profile_fields(IngressProfileFields {
        name: input
            .name
            .as_deref()
            .map(|value| normalize_non_blank("name", value))
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
        access_key: input.access_key.unwrap_or_else(|| {
            if same_driver_type {
                existing.access_key.clone()
            } else {
                String::new()
            }
        }),
        secret_key: input.secret_key.unwrap_or_else(|| {
            if same_driver_type {
                existing.secret_key.clone()
            } else {
                String::new()
            }
        }),
        base_path: input.base_path.unwrap_or_else(|| {
            if same_driver_type {
                existing.base_path.clone()
            } else {
                ".".to_string()
            }
        }),
        max_file_size: input.max_file_size.unwrap_or(existing.max_file_size),
        is_default: input.is_default,
    })
}

pub(in crate::services::managed_ingress_profile_service) fn new_profile_key() -> String {
    format!("igp_{}", crate::utils::id::new_short_token())
}

fn normalize_profile_fields(fields: IngressProfileFields) -> Result<NormalizedIngressProfileInput> {
    let IngressProfileFields {
        name,
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
        max_file_size,
        is_default,
    } = fields;

    if max_file_size < 0 {
        return Err(AsterError::validation_error(
            "max_file_size must be non-negative",
        ));
    }

    let normalized = normalize_driver_fields(ManagedIngressDriverFields {
        driver_type,
        endpoint,
        bucket,
        access_key,
        secret_key,
        base_path,
    })?;

    Ok(NormalizedIngressProfileInput {
        name,
        driver_type: normalized.driver_type,
        endpoint: normalized.endpoint,
        bucket: normalized.bucket,
        access_key: normalized.access_key,
        secret_key: normalized.secret_key,
        base_path: normalized.base_path,
        max_file_size,
        is_default,
    })
}

fn normalize_non_blank(field: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{field} cannot be blank"
        )));
    }
    Ok(trimmed.to_string())
}
