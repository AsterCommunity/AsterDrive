use std::path::Path;
use std::sync::Arc;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::{managed_ingress_profile, storage_policy};
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket;
use crate::storage::drivers::{local::LocalDriver, s3::S3Driver};
use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::paths::{normalize_relative_local_path, resolve_managed_local_path};

pub(in crate::services::managed_ingress_profile_service) struct ManagedIngressDriverFields {
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
}

pub(in crate::services::managed_ingress_profile_service) struct NormalizedManagedIngressDriverFields
{
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ManagedIngressDriverFieldKind {
    Text,
    Secret,
    Boolean,
    Number,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ManagedIngressDriverFieldDescriptor {
    pub name: String,
    pub kind: ManagedIngressDriverFieldKind,
    pub required: bool,
    pub secret: bool,
    pub label_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ManagedIngressDriverDescriptor {
    pub driver_type: DriverType,
    pub label_key: String,
    pub description_key: String,
    pub fields: Vec<ManagedIngressDriverFieldDescriptor>,
}

fn managed_ingress_text_field(
    name: &str,
    label_key: &str,
    placeholder: Option<&str>,
    help_key: Option<&str>,
    required: bool,
    secret: bool,
) -> ManagedIngressDriverFieldDescriptor {
    ManagedIngressDriverFieldDescriptor {
        name: name.to_string(),
        kind: if secret {
            ManagedIngressDriverFieldKind::Secret
        } else {
            ManagedIngressDriverFieldKind::Text
        },
        required,
        secret,
        label_key: label_key.to_string(),
        placeholder: placeholder.map(str::to_string),
        help_key: help_key.map(str::to_string),
    }
}

fn managed_ingress_number_field(
    name: &str,
    label_key: &str,
    placeholder: Option<&str>,
    help_key: Option<&str>,
    required: bool,
) -> ManagedIngressDriverFieldDescriptor {
    ManagedIngressDriverFieldDescriptor {
        name: name.to_string(),
        kind: ManagedIngressDriverFieldKind::Number,
        required,
        secret: false,
        label_key: label_key.to_string(),
        placeholder: placeholder.map(str::to_string),
        help_key: help_key.map(str::to_string),
    }
}

fn managed_ingress_boolean_field(
    name: &str,
    label_key: &str,
    help_key: Option<&str>,
    required: bool,
) -> ManagedIngressDriverFieldDescriptor {
    ManagedIngressDriverFieldDescriptor {
        name: name.to_string(),
        kind: ManagedIngressDriverFieldKind::Boolean,
        required,
        secret: false,
        label_key: label_key.to_string(),
        placeholder: None,
        help_key: help_key.map(str::to_string),
    }
}

trait ManagedIngressDriverConnector {
    fn driver_type() -> DriverType;

    fn descriptor() -> ManagedIngressDriverDescriptor;

    fn normalize_fields(
        fields: ManagedIngressDriverFields,
    ) -> Result<NormalizedManagedIngressDriverFields>;

    fn policy_base_path<S: FollowerRuntimeState>(
        state: &S,
        profile: &managed_ingress_profile::Model,
    ) -> Result<String>;

    fn validate_policy(policy: &storage_policy::Model) -> Result<()>;

    fn build_driver(policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>>;
}

struct LocalManagedIngressDriverConnector;

impl ManagedIngressDriverConnector for LocalManagedIngressDriverConnector {
    fn driver_type() -> DriverType {
        DriverType::Local
    }

    fn descriptor() -> ManagedIngressDriverDescriptor {
        ManagedIngressDriverDescriptor {
            driver_type: Self::driver_type(),
            label_key: "remote_node_ingress_profile_driver_local".to_string(),
            description_key: "remote_node_ingress_profile_local_scope_hint".to_string(),
            fields: vec![
                managed_ingress_text_field(
                    "base_path",
                    "base_path",
                    Some("tenant-a/incoming"),
                    Some("remote_node_ingress_profile_local_path_hint"),
                    true,
                    false,
                ),
                managed_ingress_number_field(
                    "max_file_size",
                    "max_file_size",
                    Some("0"),
                    Some("remote_node_ingress_profile_max_file_size_hint"),
                    false,
                ),
                managed_ingress_boolean_field(
                    "is_default",
                    "remote_node_ingress_profile_default_toggle",
                    Some("remote_node_ingress_profile_default_hint"),
                    false,
                ),
            ],
        }
    }

    fn normalize_fields(
        fields: ManagedIngressDriverFields,
    ) -> Result<NormalizedManagedIngressDriverFields> {
        Ok(NormalizedManagedIngressDriverFields {
            driver_type: Self::driver_type(),
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: normalize_relative_local_path(&fields.base_path)?,
        })
    }

    fn policy_base_path<S: FollowerRuntimeState>(
        state: &S,
        profile: &managed_ingress_profile::Model,
    ) -> Result<String> {
        Ok(resolve_managed_local_path(
            &state.config().server.follower.managed_ingress_local_root,
            &profile.base_path,
        )?
        .to_string_lossy()
        .into_owned())
    }

    fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        let base_path = Path::new(&policy.base_path);
        std::fs::create_dir_all(base_path).map_aster_err_ctx(
            &format!(
                "create managed ingress local path '{}'",
                base_path.display()
            ),
            AsterError::storage_driver_error,
        )
    }

    fn build_driver(policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        Self::validate_policy(policy)?;
        Ok(Arc::new(LocalDriver::new(policy)?))
    }
}

struct S3ManagedIngressDriverConnector;

impl ManagedIngressDriverConnector for S3ManagedIngressDriverConnector {
    fn driver_type() -> DriverType {
        DriverType::S3
    }

    fn descriptor() -> ManagedIngressDriverDescriptor {
        ManagedIngressDriverDescriptor {
            driver_type: Self::driver_type(),
            label_key: "remote_node_ingress_profile_driver_s3".to_string(),
            description_key: "remote_node_ingress_profile_s3_path_hint".to_string(),
            fields: vec![
                managed_ingress_text_field(
                    "endpoint",
                    "endpoint",
                    Some("https://s3.example.com"),
                    None,
                    true,
                    false,
                ),
                managed_ingress_text_field("bucket", "bucket", None, None, true, false),
                managed_ingress_text_field("access_key", "access_key", None, None, true, false),
                managed_ingress_text_field("secret_key", "secret_key", None, None, true, true),
                managed_ingress_text_field(
                    "base_path",
                    "base_path",
                    Some("prefix"),
                    Some("remote_node_ingress_profile_s3_path_hint"),
                    false,
                    false,
                ),
                managed_ingress_number_field(
                    "max_file_size",
                    "max_file_size",
                    Some("0"),
                    Some("remote_node_ingress_profile_max_file_size_hint"),
                    false,
                ),
                managed_ingress_boolean_field(
                    "is_default",
                    "remote_node_ingress_profile_default_toggle",
                    Some("remote_node_ingress_profile_default_hint"),
                    false,
                ),
            ],
        }
    }

    fn normalize_fields(
        fields: ManagedIngressDriverFields,
    ) -> Result<NormalizedManagedIngressDriverFields> {
        let normalized = normalize_s3_endpoint_and_bucket(&fields.endpoint, &fields.bucket)
            .map_err(|error| error.into_aster_error())?;
        Ok(NormalizedManagedIngressDriverFields {
            driver_type: Self::driver_type(),
            endpoint: normalized.endpoint,
            bucket: normalized.bucket,
            access_key: normalize_non_blank("access_key", &fields.access_key)?,
            secret_key: normalize_non_blank("secret_key", &fields.secret_key)?,
            base_path: fields.base_path.trim().trim_matches('/').to_string(),
        })
    }

    fn policy_base_path<S: FollowerRuntimeState>(
        _state: &S,
        profile: &managed_ingress_profile::Model,
    ) -> Result<String> {
        Ok(profile.base_path.clone())
    }

    fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3Driver::validate_policy(policy)
    }

    fn build_driver(policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        Ok(Arc::new(S3Driver::new(policy)?))
    }
}

struct ManagedIngressDriverRegistration {
    driver_type: DriverType,
    connector: BuiltinManagedIngressDriverConnector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinManagedIngressDriverConnector {
    Local,
    S3,
}

impl BuiltinManagedIngressDriverConnector {
    fn descriptor(self) -> ManagedIngressDriverDescriptor {
        match self {
            Self::Local => LocalManagedIngressDriverConnector::descriptor(),
            Self::S3 => S3ManagedIngressDriverConnector::descriptor(),
        }
    }

    fn normalize_fields(
        self,
        fields: ManagedIngressDriverFields,
    ) -> Result<NormalizedManagedIngressDriverFields> {
        match self {
            Self::Local => LocalManagedIngressDriverConnector::normalize_fields(fields),
            Self::S3 => S3ManagedIngressDriverConnector::normalize_fields(fields),
        }
    }

    fn policy_base_path<S: FollowerRuntimeState>(
        self,
        state: &S,
        profile: &managed_ingress_profile::Model,
    ) -> Result<String> {
        match self {
            Self::Local => LocalManagedIngressDriverConnector::policy_base_path(state, profile),
            Self::S3 => S3ManagedIngressDriverConnector::policy_base_path(state, profile),
        }
    }

    fn validate_policy(self, policy: &storage_policy::Model) -> Result<()> {
        match self {
            Self::Local => LocalManagedIngressDriverConnector::validate_policy(policy),
            Self::S3 => S3ManagedIngressDriverConnector::validate_policy(policy),
        }
    }

    fn build_driver(self, policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        match self {
            Self::Local => LocalManagedIngressDriverConnector::build_driver(policy),
            Self::S3 => S3ManagedIngressDriverConnector::build_driver(policy),
        }
    }
}

static MANAGED_INGRESS_DRIVER_REGISTRATIONS: &[ManagedIngressDriverRegistration] = &[
    ManagedIngressDriverRegistration {
        driver_type: DriverType::Local,
        connector: BuiltinManagedIngressDriverConnector::Local,
    },
    ManagedIngressDriverRegistration {
        driver_type: DriverType::S3,
        connector: BuiltinManagedIngressDriverConnector::S3,
    },
];

fn registration_for(driver_type: DriverType) -> Result<&'static ManagedIngressDriverRegistration> {
    MANAGED_INGRESS_DRIVER_REGISTRATIONS
        .iter()
        .find(|registration| registration.driver_type == driver_type)
        .ok_or_else(|| managed_ingress_unsupported_driver_error(driver_type))
}

pub(crate) fn registered_managed_ingress_driver_types() -> Vec<DriverType> {
    MANAGED_INGRESS_DRIVER_REGISTRATIONS
        .iter()
        .map(|registration| registration.driver_type)
        .collect()
}

#[cfg(test)]
pub(crate) fn list_registered_managed_ingress_driver_descriptors()
-> Vec<ManagedIngressDriverDescriptor> {
    MANAGED_INGRESS_DRIVER_REGISTRATIONS
        .iter()
        .map(|registration| registration.connector.descriptor())
        .collect()
}

pub fn managed_ingress_driver_descriptor(
    driver_type: DriverType,
) -> Result<ManagedIngressDriverDescriptor> {
    Ok(registration_for(driver_type)?.connector.descriptor())
}

pub(in crate::services::managed_ingress_profile_service) fn normalize_driver_fields(
    fields: ManagedIngressDriverFields,
) -> Result<NormalizedManagedIngressDriverFields> {
    registration_for(fields.driver_type)?
        .connector
        .normalize_fields(fields)
}

pub(in crate::services::managed_ingress_profile_service) fn validate_driver_from_profile<
    S: FollowerRuntimeState,
>(
    state: &S,
    profile: &managed_ingress_profile::Model,
) -> Result<()> {
    let registration = registration_for(profile.driver_type)?;
    let policy = build_policy_model(state, profile, registration)?;
    registration.connector.validate_policy(&policy)
}

pub(in crate::services::managed_ingress_profile_service) fn build_driver_from_profile<
    S: FollowerRuntimeState,
>(
    state: &S,
    profile: &managed_ingress_profile::Model,
) -> Result<Arc<dyn StorageDriver>> {
    let registration = registration_for(profile.driver_type)?;
    let policy = build_policy_model(state, profile, registration)?;
    registration.connector.build_driver(&policy)
}

fn managed_ingress_unsupported_driver_error(driver_type: DriverType) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::ManagedIngressDriverUnsupported,
        format!(
            "managed ingress profiles do not support the {} driver",
            driver_type.as_str()
        ),
    )
}

fn build_policy_model<S: FollowerRuntimeState>(
    state: &S,
    profile: &managed_ingress_profile::Model,
    registration: &ManagedIngressDriverRegistration,
) -> Result<storage_policy::Model> {
    let base_path = registration.connector.policy_base_path(state, profile)?;

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

fn normalize_non_blank(field: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{field} cannot be blank"
        )));
    }
    Ok(trimmed.to_string())
}
