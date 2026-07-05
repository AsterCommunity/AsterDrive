use std::path::Path;
use std::sync::Arc;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::{remote_storage_target, storage_policy};
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::storage::StorageDriver;
use crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket;
use crate::storage::drivers::{local::LocalDriver, s3::S3Driver};
use crate::storage::field_contract::{
    StorageDescriptorFieldKind, StorageDescriptorFieldSemantics, normalize_object_storage_prefix,
    normalize_required_storage_field,
};
use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::paths::{normalize_relative_local_path, resolve_remote_storage_target_local_path};

pub(in crate::services::remote_storage_target_service) struct RemoteStorageTargetDriverFields {
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
}

pub(in crate::services::remote_storage_target_service) struct NormalizedRemoteStorageTargetDriverFields
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
pub enum RemoteStorageTargetDriverFieldKind {
    Text,
    Secret,
    Boolean,
    Number,
}

impl TryFrom<StorageDescriptorFieldKind> for RemoteStorageTargetDriverFieldKind {
    type Error = &'static str;

    fn try_from(value: StorageDescriptorFieldKind) -> std::result::Result<Self, Self::Error> {
        match value {
            StorageDescriptorFieldKind::Text => Ok(Self::Text),
            StorageDescriptorFieldKind::Secret => Ok(Self::Secret),
            StorageDescriptorFieldKind::Boolean => Ok(Self::Boolean),
            StorageDescriptorFieldKind::Number => Ok(Self::Number),
            StorageDescriptorFieldKind::Select => {
                Err("managed ingress target descriptors do not support select fields")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetDriverFieldValidation {
    pub relative_local_path: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetDriverFieldDescriptor {
    pub name: String,
    pub kind: RemoteStorageTargetDriverFieldKind,
    pub required: bool,
    pub secret: bool,
    pub label_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<RemoteStorageTargetDriverFieldValidation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetDriverDescriptor {
    pub driver_type: DriverType,
    pub label_key: String,
    pub description_key: String,
    pub fields: Vec<RemoteStorageTargetDriverFieldDescriptor>,
}

fn remote_storage_target_text_field(
    name: &str,
    label_key: &str,
    placeholder: Option<&str>,
    help_key: Option<&str>,
    required: bool,
    secret: bool,
) -> RemoteStorageTargetDriverFieldDescriptor {
    remote_storage_target_text_field_with_validation(
        name,
        label_key,
        placeholder,
        help_key,
        required,
        secret,
        None,
    )
}

fn remote_storage_target_text_field_with_validation(
    name: &str,
    label_key: &str,
    placeholder: Option<&str>,
    help_key: Option<&str>,
    required: bool,
    secret: bool,
    validation: Option<RemoteStorageTargetDriverFieldValidation>,
) -> RemoteStorageTargetDriverFieldDescriptor {
    let semantics = if secret {
        StorageDescriptorFieldSemantics::secret(required)
    } else {
        StorageDescriptorFieldSemantics::text(required)
    };
    RemoteStorageTargetDriverFieldDescriptor {
        name: name.to_string(),
        kind: semantics
            .kind
            .try_into()
            .expect("text/secret storage target fields must map to managed ingress field kinds"),
        required: semantics.required,
        secret: semantics.secret,
        label_key: label_key.to_string(),
        placeholder: placeholder.map(str::to_string),
        help_key: help_key.map(str::to_string),
        validation,
    }
}

fn remote_storage_target_boolean_field(
    name: &str,
    label_key: &str,
    help_key: Option<&str>,
    required: bool,
) -> RemoteStorageTargetDriverFieldDescriptor {
    let semantics = StorageDescriptorFieldSemantics::boolean(required);
    RemoteStorageTargetDriverFieldDescriptor {
        name: name.to_string(),
        kind: semantics
            .kind
            .try_into()
            .expect("boolean storage target fields must map to managed ingress field kinds"),
        required: semantics.required,
        secret: semantics.secret,
        label_key: label_key.to_string(),
        placeholder: None,
        help_key: help_key.map(str::to_string),
        validation: None,
    }
}

trait RemoteStorageTargetDriverConnector {
    fn driver_type() -> DriverType;

    fn descriptor() -> RemoteStorageTargetDriverDescriptor;

    fn normalize_fields(
        fields: RemoteStorageTargetDriverFields,
    ) -> Result<NormalizedRemoteStorageTargetDriverFields>;

    fn policy_base_path<S: FollowerRuntimeState>(
        state: &S,
        target: &remote_storage_target::Model,
    ) -> Result<String>;

    fn validate_policy(policy: &storage_policy::Model) -> Result<()>;

    fn build_driver(policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>>;
}

struct LocalRemoteStorageTargetDriverConnector;

impl RemoteStorageTargetDriverConnector for LocalRemoteStorageTargetDriverConnector {
    fn driver_type() -> DriverType {
        DriverType::Local
    }

    fn descriptor() -> RemoteStorageTargetDriverDescriptor {
        RemoteStorageTargetDriverDescriptor {
            driver_type: Self::driver_type(),
            label_key: "remote_node_ingress_profile_driver_local".to_string(),
            description_key: "remote_node_ingress_profile_local_scope_hint".to_string(),
            fields: vec![
                remote_storage_target_text_field_with_validation(
                    "base_path",
                    "base_path",
                    Some("tenant-a/incoming"),
                    Some("remote_node_ingress_profile_local_path_hint"),
                    true,
                    false,
                    Some(RemoteStorageTargetDriverFieldValidation {
                        relative_local_path: true,
                    }),
                ),
                remote_storage_target_boolean_field(
                    "is_default",
                    "remote_node_ingress_profile_default_toggle",
                    Some("remote_node_ingress_profile_default_hint"),
                    false,
                ),
            ],
        }
    }

    fn normalize_fields(
        fields: RemoteStorageTargetDriverFields,
    ) -> Result<NormalizedRemoteStorageTargetDriverFields> {
        Ok(NormalizedRemoteStorageTargetDriverFields {
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
        target: &remote_storage_target::Model,
    ) -> Result<String> {
        Ok(resolve_remote_storage_target_local_path(
            &state
                .config()
                .server
                .follower
                .remote_storage_target_local_root,
            &target.base_path,
        )?
        .to_string_lossy()
        .into_owned())
    }

    fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        let base_path = Path::new(&policy.base_path);
        std::fs::create_dir_all(base_path).map_aster_err_ctx(
            &format!(
                "create remote storage target local path '{}'",
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

struct S3RemoteStorageTargetDriverConnector;

impl RemoteStorageTargetDriverConnector for S3RemoteStorageTargetDriverConnector {
    fn driver_type() -> DriverType {
        DriverType::S3
    }

    fn descriptor() -> RemoteStorageTargetDriverDescriptor {
        RemoteStorageTargetDriverDescriptor {
            driver_type: Self::driver_type(),
            label_key: "remote_node_ingress_profile_driver_s3".to_string(),
            description_key: "remote_node_ingress_profile_s3_path_hint".to_string(),
            fields: vec![
                remote_storage_target_text_field(
                    "endpoint",
                    "endpoint",
                    Some("https://s3.example.com"),
                    None,
                    true,
                    false,
                ),
                remote_storage_target_text_field("bucket", "bucket", None, None, true, false),
                remote_storage_target_text_field(
                    "access_key",
                    "access_key",
                    None,
                    None,
                    true,
                    false,
                ),
                remote_storage_target_text_field(
                    "secret_key",
                    "secret_key",
                    None,
                    None,
                    true,
                    true,
                ),
                remote_storage_target_text_field(
                    "base_path",
                    "base_path",
                    Some("prefix"),
                    Some("remote_node_ingress_profile_s3_path_hint"),
                    false,
                    false,
                ),
                remote_storage_target_boolean_field(
                    "is_default",
                    "remote_node_ingress_profile_default_toggle",
                    Some("remote_node_ingress_profile_default_hint"),
                    false,
                ),
            ],
        }
    }

    fn normalize_fields(
        fields: RemoteStorageTargetDriverFields,
    ) -> Result<NormalizedRemoteStorageTargetDriverFields> {
        let normalized = normalize_s3_endpoint_and_bucket(&fields.endpoint, &fields.bucket)
            .map_err(|error| error.into_aster_error())?;
        Ok(NormalizedRemoteStorageTargetDriverFields {
            driver_type: Self::driver_type(),
            endpoint: normalized.endpoint,
            bucket: normalized.bucket,
            access_key: normalize_required_storage_field("access_key", &fields.access_key)?,
            secret_key: normalize_required_storage_field("secret_key", &fields.secret_key)?,
            base_path: normalize_object_storage_prefix(&fields.base_path),
        })
    }

    fn policy_base_path<S: FollowerRuntimeState>(
        _state: &S,
        target: &remote_storage_target::Model,
    ) -> Result<String> {
        Ok(target.base_path.clone())
    }

    fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3Driver::validate_policy(policy)
    }

    fn build_driver(policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        Ok(Arc::new(S3Driver::new(policy)?))
    }
}

struct RemoteStorageTargetDriverRegistration {
    driver_type: DriverType,
    connector: BuiltinRemoteStorageTargetDriverConnector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinRemoteStorageTargetDriverConnector {
    Local,
    S3,
}

impl BuiltinRemoteStorageTargetDriverConnector {
    fn descriptor(self) -> RemoteStorageTargetDriverDescriptor {
        match self {
            Self::Local => LocalRemoteStorageTargetDriverConnector::descriptor(),
            Self::S3 => S3RemoteStorageTargetDriverConnector::descriptor(),
        }
    }

    fn normalize_fields(
        self,
        fields: RemoteStorageTargetDriverFields,
    ) -> Result<NormalizedRemoteStorageTargetDriverFields> {
        match self {
            Self::Local => LocalRemoteStorageTargetDriverConnector::normalize_fields(fields),
            Self::S3 => S3RemoteStorageTargetDriverConnector::normalize_fields(fields),
        }
    }

    fn policy_base_path<S: FollowerRuntimeState>(
        self,
        state: &S,
        target: &remote_storage_target::Model,
    ) -> Result<String> {
        match self {
            Self::Local => LocalRemoteStorageTargetDriverConnector::policy_base_path(state, target),
            Self::S3 => S3RemoteStorageTargetDriverConnector::policy_base_path(state, target),
        }
    }

    fn validate_policy(self, policy: &storage_policy::Model) -> Result<()> {
        match self {
            Self::Local => LocalRemoteStorageTargetDriverConnector::validate_policy(policy),
            Self::S3 => S3RemoteStorageTargetDriverConnector::validate_policy(policy),
        }
    }

    fn build_driver(self, policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        match self {
            Self::Local => LocalRemoteStorageTargetDriverConnector::build_driver(policy),
            Self::S3 => S3RemoteStorageTargetDriverConnector::build_driver(policy),
        }
    }
}

static REMOTE_STORAGE_TARGET_DRIVER_REGISTRATIONS: &[RemoteStorageTargetDriverRegistration] = &[
    RemoteStorageTargetDriverRegistration {
        driver_type: DriverType::Local,
        connector: BuiltinRemoteStorageTargetDriverConnector::Local,
    },
    RemoteStorageTargetDriverRegistration {
        driver_type: DriverType::S3,
        connector: BuiltinRemoteStorageTargetDriverConnector::S3,
    },
];

fn registration_for(
    driver_type: DriverType,
) -> Result<&'static RemoteStorageTargetDriverRegistration> {
    REMOTE_STORAGE_TARGET_DRIVER_REGISTRATIONS
        .iter()
        .find(|registration| registration.driver_type == driver_type)
        .ok_or_else(|| remote_storage_target_unsupported_driver_error(driver_type))
}

pub(crate) fn registered_remote_storage_target_driver_types() -> Vec<DriverType> {
    REMOTE_STORAGE_TARGET_DRIVER_REGISTRATIONS
        .iter()
        .map(|registration| registration.driver_type)
        .collect()
}

#[cfg(test)]
pub(crate) fn list_registered_remote_storage_target_driver_descriptors()
-> Vec<RemoteStorageTargetDriverDescriptor> {
    REMOTE_STORAGE_TARGET_DRIVER_REGISTRATIONS
        .iter()
        .map(|registration| registration.connector.descriptor())
        .collect()
}

pub fn remote_storage_target_driver_descriptor(
    driver_type: DriverType,
) -> Result<RemoteStorageTargetDriverDescriptor> {
    Ok(registration_for(driver_type)?.connector.descriptor())
}

pub(in crate::services::remote_storage_target_service) fn normalize_driver_fields(
    fields: RemoteStorageTargetDriverFields,
) -> Result<NormalizedRemoteStorageTargetDriverFields> {
    registration_for(fields.driver_type)?
        .connector
        .normalize_fields(fields)
}

pub(in crate::services::remote_storage_target_service) fn validate_driver_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<()> {
    let registration = registration_for(target.driver_type)?;
    let policy = build_policy_model(state, target, registration)?;
    registration.connector.validate_policy(&policy)
}

pub(in crate::services::remote_storage_target_service) fn build_driver_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<Arc<dyn StorageDriver>> {
    let registration = registration_for(target.driver_type)?;
    let policy = build_policy_model(state, target, registration)?;
    registration.connector.build_driver(&policy)
}

fn remote_storage_target_unsupported_driver_error(driver_type: DriverType) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::ManagedIngressDriverUnsupported,
        format!(
            "managed remote storage targets do not support the {} driver",
            driver_type.as_str()
        ),
    )
}

fn build_policy_model<S: FollowerRuntimeState>(
    state: &S,
    target: &remote_storage_target::Model,
    registration: &RemoteStorageTargetDriverRegistration,
) -> Result<storage_policy::Model> {
    let base_path = registration.connector.policy_base_path(state, target)?;

    Ok(storage_policy::Model {
        id: target.id,
        name: target.name.clone(),
        driver_type: target.driver_type,
        endpoint: target.endpoint.clone(),
        bucket: target.bucket.clone(),
        access_key: target.access_key.clone(),
        secret_key: target.secret_key.clone(),
        base_path,
        remote_node_id: None,
        remote_storage_target_key: None,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        options: StoredStoragePolicyOptions::empty(),
        is_default: target.is_default,
        chunk_size: 0,
        created_at: target.created_at,
        updated_at: target.updated_at,
    })
}
