use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::StoredStoragePolicyAllowedTypes;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionId,
    StorageConnectorActionKind, StorageConnectorDescriptor, StorageConnectorFieldDescriptor,
    StorageConnectorFieldScope, StorageConnectorSelectOptionInput, storage_connector_select_field,
};
use aster_drive_storage::{
    ConnectorConfigEnvelope, ConnectorId, StorageConnectorActionSchema,
    StorageConnectorFieldDefaultValue, StorageDriver, StorageErrorKind,
    StoragePolicyBehaviorConfig,
};
use serde::{Serialize, de::DeserializeOwned};

use super::StorageConnectorCredentialInput;

const STATIC_CREDENTIAL_CLEANUP_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticCredentialCleanupSnapshotV1 {
    ciphertext: String,
}

/// Messages owned by descriptor helpers shared by multiple connector plugins.
///
/// Each connector explicitly composes this slice with its own resource and the
/// localization builder only keeps ids referenced by that connector's
/// descriptor, so bundles never expose another connector's private copy.
pub(super) const LOCALIZATION_MESSAGES:
    &[aster_drive_storage::StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("base_path", "Base Path", "基础路径"),
    aster_drive_storage::storage_connector_message!("bucket", "Bucket", "存储桶"),
    aster_drive_storage::storage_connector_message!(
        "content_dedup",
        "Content Deduplication",
        "内容去重",
    ),
    aster_drive_storage::storage_connector_message!("endpoint", "Endpoint", "端点"),
    aster_drive_storage::storage_connector_message!(
        "download_strategy_presigned",
        "Presigned Redirect",
        "Presigned 重定向",
    ),
    aster_drive_storage::storage_connector_message!(
        "download_strategy_presigned_desc",
        "After AsterDrive completes permission checks, it redirects the browser to a short-lived GET URL from the storage backend. This reduces app-node download bandwidth, but the backend serves the final response and cache behavior.",
        "AsterDrive 完成权限校验后，返回一个短时效的存储后端 GET URL 重定向。这样能减少应用节点的下载带宽压力，但最终响应头和缓存行为会由存储后端承担。",
    ),
    aster_drive_storage::storage_connector_message!(
        "download_strategy_relay_stream",
        "Server Relay Download",
        "服务端中继下载",
    ),
    aster_drive_storage::storage_connector_message!(
        "download_strategy_relay_stream_desc",
        "AsterDrive fetches the object from the storage backend and streams it back to the browser. Use this when you need the app node to fully control response headers, same-origin delivery, or downstream network policy.",
        "AsterDrive 先从存储后端拉取对象，再把内容流式回传给浏览器。适合需要由应用节点完全控制响应头、同源下载行为或下游网络策略的场景。",
    ),
    aster_drive_storage::storage_connector_message!(
        "object_storage_download_strategy",
        "Object Storage Download Strategy",
        "对象存储下载方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "object_storage_upload_strategy",
        "Object Storage Upload Strategy",
        "对象存储上传方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_object_storage_desc",
        "Test object storage connections before saving. Blank secret fields keep the current credentials.",
        "对象存储策略保存前建议测试连接；留空密钥字段会保留现有凭证。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_object_storage_helper",
        "Connection tests and upload strategy are available after the basic connection is filled in.",
        "基础连接填好后，可以在下一步测试连接并选择上传策略。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_connection_desc",
        "Review the connection settings required by this storage backend.",
        "检查这个存储后端需要的连接配置。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_connection_title",
        "Configure Connection",
        "配置连接",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_object_storage_connection_desc",
        "Set the object-storage endpoint, bucket, and credentials.",
        "填写对象存储 endpoint、bucket 和访问凭证。",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_endpoint_protocol_required_error",
        "S3 endpoint must include http:// or https://.",
        "S3 endpoint 必须包含 http:// 或 https://。",
    ),
    aster_drive_storage::storage_connector_message!(
        "test_connection",
        "Test Connection",
        "测试连接",
    ),
    aster_drive_storage::storage_connector_message!(
        "upload_strategy_presigned",
        "Presigned Direct Upload",
        "Presigned 直传",
    ),
    aster_drive_storage::storage_connector_message!(
        "upload_strategy_presigned_desc",
        "Upload directly to the storage backend via presigned URLs. Files ≤ chunk size use a single PUT; larger files use multipart direct upload with resume support. This path does not perform SHA256 deduplication and requires the backend to expose the `ETag` response header.",
        "浏览器通过 presigned URL 直接上传到存储后端。文件 ≤ 分片大小时单次 PUT；更大文件自动使用 multipart 直传（支持断点续传）。该路径不做 SHA256 去重，并要求存储后端暴露 `ETag` 响应头。",
    ),
    aster_drive_storage::storage_connector_message!(
        "upload_strategy_relay_stream",
        "Server Relay Stream",
        "服务端流式中继",
    ),
    aster_drive_storage::storage_connector_message!(
        "upload_strategy_relay_stream_desc",
        "The browser uploads to AsterDrive, and the server relays the bytes directly to the storage backend. The normal path does not write local temporary files; only a small fallback path uses temp files. relay_stream does not perform SHA256 deduplication.",
        "浏览器把文件上传到 AsterDrive，服务端直接中继到存储后端。正常路径不落本机临时文件；只有少数 fallback 场景才会使用临时文件。relay_stream 不做 SHA256 去重。",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StorageTransferDirection {
    Upload,
    Download,
}

/// Build the common relay/presigned transfer strategy field.
///
/// Direction is explicit because a boolean argument makes call sites such as
/// `transfer_options(true)` impossible to review without opening the helper.
pub(super) fn transfer_strategy_field(
    name: &str,
    direction: StorageTransferDirection,
) -> StorageConnectorFieldDescriptor {
    let mut field = storage_connector_select_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        true,
        transfer_strategy_options(direction),
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        "relay_stream".to_string(),
    ));
    field
}

fn transfer_strategy_options(
    direction: StorageTransferDirection,
) -> Vec<StorageConnectorSelectOptionInput<'static>> {
    let (relay_label, relay_description, presigned_label, presigned_description) = match direction {
        StorageTransferDirection::Upload => (
            "upload_strategy_relay_stream",
            "upload_strategy_relay_stream_desc",
            "upload_strategy_presigned",
            "upload_strategy_presigned_desc",
        ),
        StorageTransferDirection::Download => (
            "download_strategy_relay_stream",
            "download_strategy_relay_stream_desc",
            "download_strategy_presigned",
            "download_strategy_presigned_desc",
        ),
    };
    vec![
        StorageConnectorSelectOptionInput {
            value: "relay_stream",
            label_key: relay_label,
            description_key: Some(relay_description),
        },
        StorageConnectorSelectOptionInput {
            value: "presigned",
            label_key: presigned_label,
            description_key: Some(presigned_description),
        },
    ]
}

pub(super) fn runtime_static_credential<T: DeserializeOwned>(
    registry: &crate::storage::DriverRegistry,
    policy: &storage_policy::Model,
    connector_id: &'static str,
) -> Result<T> {
    let credential = registry.get_runtime_credential(policy.id).ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Auth,
            format!("storage policy {} is missing static credentials", policy.id),
        )
    })?;
    let values = credential.require::<serde_json::Value>(connector_id)?;
    serde_json::from_value(values.clone()).map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "storage policy {} has invalid static credentials: {error}",
                policy.id
            ),
        )
    })
}

/// Preserve an encrypted static credential for cleanup work that may run after
/// the policy and its credential row have been deleted.
pub(super) async fn static_credential_cleanup_snapshot(
    context: &super::StorageConnectorContext<'_>,
    policy: &storage_policy::Model,
    connector_id: &str,
    credential_schema_version: u32,
) -> Result<Option<super::StoragePolicyCleanupDriverSnapshot>> {
    let Some(credential) =
        crate::db::repository::storage_policy_connector_credential_repo::find_by_policy(
            context.writer_db(),
            policy.id,
        )
        .await?
    else {
        return Ok(None);
    };
    let expected_schema_version = i32::try_from(credential_schema_version).map_err(|_| {
        AsterError::database_operation("connector schema version exceeds database range")
    })?;
    if credential.connector_id != connector_id
        || credential.schema_version != expected_schema_version
    {
        return Err(AsterError::database_operation(format!(
            "storage policy {} credential does not match connector cleanup schema",
            policy.id
        )));
    }
    super::StoragePolicyCleanupDriverSnapshot::encode(
        ConnectorId::declared(connector_id),
        STATIC_CREDENTIAL_CLEANUP_SNAPSHOT_SCHEMA_VERSION,
        &StaticCredentialCleanupSnapshotV1 {
            ciphertext: credential.ciphertext,
        },
    )
    .map(Some)
}

/// Decode a static credential from the encrypted snapshot stored in a delayed
/// cleanup task. The original connector-credential AAD remains valid because
/// the task also preserves the policy id, connector id, and schema version.
pub(super) fn static_credential_from_cleanup_snapshot<T: DeserializeOwned>(
    context: &super::StorageConnectorContext<'_>,
    policy: &storage_policy::Model,
    snapshots: super::StoragePolicyCleanupSnapshots<'_>,
    connector_id: &str,
    credential_schema_version: u32,
) -> Result<T> {
    let snapshot = snapshots.driver_snapshot.ok_or_else(|| {
        AsterError::database_operation(format!(
            "storage policy {} cleanup task is missing encrypted credentials for connector '{}'",
            policy.id, connector_id
        ))
    })?;
    let payload: StaticCredentialCleanupSnapshotV1 = snapshot.decode(
        connector_id,
        STATIC_CREDENTIAL_CLEANUP_SNAPSHOT_SCHEMA_VERSION,
    )?;
    let plaintext =
        crate::services::storage_policy::credential::crypto::decrypt_connector_credential(
            &context.config().auth.storage_credential_secret_key,
            policy.id,
            connector_id,
            credential_schema_version,
            &payload.ciphertext,
        )?;
    serde_json::from_str(&plaintext).map_err(|error| {
        AsterError::database_operation(format!(
            "storage policy {} cleanup credential for connector '{}' is invalid: {error}",
            policy.id, connector_id
        ))
    })
}

pub(super) fn build_connection_test_policy(
    connector_config: ConnectorConfigEnvelope,
    behavior: StoragePolicyBehaviorConfig,
) -> Result<storage_policy::Model> {
    let connector_id = connector_config.connector_id.clone();
    let connector_config = ConnectorConfigEnvelope::new(
        connector_id.clone(),
        connector_config.schema_version,
        serde_json::to_value(connector_config.values).map_err(|error| {
            AsterError::internal_error(format!("serialize draft connector config: {error}"))
        })?,
    );
    let storage_config =
        aster_drive_storage::encode_storage_policy_config(connector_config, behavior)
            .map(aster_drive_model::types::StoredStoragePolicyConfig)
            .map_err(|error| {
                AsterError::internal_error(format!("serialize storage policy config: {error}"))
            })?;
    Ok(storage_policy::Model {
        id: 0,
        name: String::new(),
        connector_id: connector_id.as_str().to_string(),
        storage_config,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        is_default: false,
        chunk_size: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
}

pub(super) fn encode_normalized_connector_config<T: Serialize>(
    connector_id: ConnectorId,
    schema_version: u32,
    values: T,
) -> Result<ConnectorConfigEnvelope> {
    let values = serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "serialize normalized connector config '{connector_id}': {error}"
            ))
        })?;
    Ok(ConnectorConfigEnvelope::new(
        connector_id,
        schema_version,
        values,
    ))
}

/// Decode one persisted policy through the connector's concrete typed schema.
///
/// Connector implementations call this at their boundary and pass plain
/// runtime values to drivers. Drivers never inspect SeaORM entities or generic
/// JSON envelopes themselves.
pub(super) fn decode_typed_policy_config<T: DeserializeOwned>(
    policy: &storage_policy::Model,
    connector_id: &'static str,
    schema_version: u32,
) -> Result<(T, StoragePolicyBehaviorConfig)> {
    decode_typed_policy_config_for_id(policy, &ConnectorId::declared(connector_id), schema_version)
}

pub(super) fn decode_typed_policy_config_for_id<T: DeserializeOwned>(
    policy: &storage_policy::Model,
    connector_id: &ConnectorId,
    schema_version: u32,
) -> Result<(T, StoragePolicyBehaviorConfig)> {
    aster_drive_storage::decode_storage_policy_config(
        policy.storage_config.as_ref(),
        connector_id,
        schema_version,
    )
    .map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "storage policy {} has invalid '{}' configuration: {error}",
                policy.id, connector_id
            ),
        )
    })
}

pub(super) fn decode_normalized_connector_config<T: DeserializeOwned>(
    config: &ConnectorConfigEnvelope,
) -> Result<T> {
    serde_json::from_value(serde_json::to_value(&config.values).map_err(|error| {
        AsterError::internal_error(format!("serialize normalized connector config: {error}"))
    })?)
    .map_err(|error| {
        AsterError::validation_error(format!(
            "invalid '{}' connector configuration: {error}",
            config.connector_id
        ))
    })
}

pub(super) fn decode_static_credential<T: DeserializeOwned>(
    credential: &StorageConnectorCredentialInput,
    connector_id: &str,
) -> Result<T> {
    let StorageConnectorCredentialInput::Static(values) = credential else {
        return Err(AsterError::validation_error(format!(
            "storage connector '{connector_id}' requires static credentials"
        )));
    };
    serde_json::from_value(values.clone()).map_err(|error| {
        AsterError::validation_error(format!(
            "invalid static credentials for storage connector '{connector_id}': {error}"
        ))
    })
}

pub(super) fn decode_authorization_application<T: DeserializeOwned>(
    credential: &StorageConnectorCredentialInput,
    connector_id: &str,
) -> Result<T> {
    let StorageConnectorCredentialInput::AuthorizationApplication(values) = credential else {
        return Err(AsterError::validation_error(format!(
            "storage connector '{connector_id}' requires authorization application credentials"
        )));
    };
    serde_json::from_value(values.clone()).map_err(|error| {
        AsterError::validation_error(format!(
            "invalid authorization application credentials for storage connector '{connector_id}': {error}"
        ))
    })
}

pub(super) fn validate_required_credential_field(
    value: &str,
    field: &str,
    connector_id: &str,
) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AsterError::validation_error(format!(
            "credential field '{field}' is required for storage connector '{connector_id}'"
        )));
    }
    Ok(())
}

/// Fill omitted or blank static credential fields from the saved encrypted payload.
///
/// Admin edit forms intentionally leave secret inputs blank when the value is
/// unchanged. The connector contract applies this object-level merge before a
/// draft connection test, without teaching orchestration code any provider
/// field names.
pub(super) fn merge_saved_static_credential(
    input: StorageConnectorCredentialInput,
    saved: serde_json::Value,
) -> Result<StorageConnectorCredentialInput> {
    let mut current = match input {
        StorageConnectorCredentialInput::None => {
            if !saved.is_object() {
                return Err(AsterError::database_operation(
                    "stored static connector credential must be a JSON object",
                ));
            }
            return Ok(StorageConnectorCredentialInput::Static(saved));
        }
        StorageConnectorCredentialInput::Static(current) => current,
        other => return Ok(other),
    };
    let Some(current_fields) = current.as_object_mut() else {
        return Ok(StorageConnectorCredentialInput::Static(current));
    };
    let saved_fields = saved.as_object().ok_or_else(|| {
        AsterError::database_operation("stored static connector credential must be a JSON object")
    })?;
    for (name, saved_value) in saved_fields {
        let should_restore = current_fields.get(name).is_none_or(|value| {
            value.is_null() || value.as_str().is_some_and(|value| value.trim().is_empty())
        });
        if should_restore {
            current_fields.insert(name.clone(), saved_value.clone());
        }
    }
    Ok(StorageConnectorCredentialInput::Static(current))
}

/// Convert the deprecated `access_key`/`secret_key` policy columns into the
/// current connector-owned static credential struct.
///
/// This helper is exclusive to the AsterDrive 0.5.0 startup migration and will
/// be completely removed together with the legacy columns in AsterDrive 0.6.0.
pub(super) fn import_legacy_static_credential<T: Serialize>(
    connector_id: &str,
    input: super::LegacyStorageConnectorCredentialInput,
    build: impl FnOnce(super::LegacyStoragePolicyStaticCredential) -> T,
) -> Result<Option<serde_json::Value>> {
    if input.application_config.is_some() || input.authorization.is_some() {
        return Err(AsterError::database_operation(format!(
            "connector '{connector_id}' received incompatible legacy authorization credentials",
        )));
    }
    let Some(mut credential) = input.static_credential else {
        return Ok(None);
    };
    credential.access_key = credential.access_key.trim().to_string();
    credential.secret_key = credential.secret_key.trim().to_string();
    if credential.access_key.is_empty() && credential.secret_key.is_empty() {
        return Ok(None);
    }
    if credential.access_key.is_empty() || credential.secret_key.is_empty() {
        return Err(AsterError::database_operation(format!(
            "connector '{connector_id}' has incomplete legacy static credentials",
        )));
    }
    serde_json::to_value(build(credential))
        .map(Some)
        .map_err(|error| {
            AsterError::database_operation(format!(
                "serialize migrated credential for connector '{connector_id}': {error}",
            ))
        })
}

pub(super) fn decode_normalized_connector_action_input<T>(
    descriptor: &StorageConnectorActionDescriptor,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<T>
where
    T: DeserializeOwned + StorageConnectorActionSchema,
{
    let declared_fields = T::action_fields();
    if descriptor.fields != declared_fields {
        return Err(AsterError::internal_error(format!(
            "storage connector action '{}' descriptor fields do not match its typed input schema",
            descriptor.action_id.as_str()
        )));
    }
    serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .map_err(|error| {
            validation_error_with_code(
                ApiErrorCode::PolicyActionParameterInvalid,
                format!(
                    "storage connector action '{}' input is invalid: {error}",
                    descriptor.action_id.as_str()
                ),
            )
        })
}

pub(super) fn unsupported_connector_action_error(
    descriptor: &StorageConnectorDescriptor,
    action_id: &StorageConnectorActionId,
) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage connector action '{}' is not supported for connector '{}'",
            action_id.as_str(),
            descriptor.connector_id.as_str()
        ),
    )
}

pub(super) fn unsupported_draft_connection_test_error(
    descriptor: StorageConnectorDescriptor,
) -> AsterError {
    if descriptor.actions.iter().any(|action| {
        action.kind == StorageConnectorActionKind::ConnectionTest
            && action
                .endpoints
                .contains(&StorageConnectorActionEndpoint::TestPolicyConnection)
            && action.requires_saved_policy
            && action.requires_authorization
    }) {
        return validation_error_with_code(
            ApiErrorCode::PolicyActionUnsupported,
            format!(
                "storage policy driver '{}' requires a saved storage policy with completed authorization; use the saved policy connection test after authorization",
                descriptor.connector_id.as_str(),
            ),
        );
    }
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy driver '{}' does not support draft connection tests",
            descriptor.connector_id.as_str(),
        ),
    )
}

pub(super) fn unsupported_saved_connection_test_error(
    descriptor: StorageConnectorDescriptor,
) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::PolicyActionUnsupported,
        format!(
            "storage policy driver '{}' does not support saved-policy connection tests",
            descriptor.connector_id.as_str(),
        ),
    )
}

pub(super) async fn probe_storage_driver(
    driver: &dyn StorageDriver,
    write_error_context: &'static str,
) -> Result<()> {
    let test_path = format!("_aster_connection_test-{}", uuid::Uuid::new_v4());
    driver
        .put(&test_path, b"ok")
        .await
        .map_aster_err_ctx(write_error_context, AsterError::storage_driver_error)?;
    driver
        .delete(&test_path)
        .await
        .inspect_err(|error| {
            tracing::warn!(path = %test_path, "failed to clean up connection test file: {error}");
        })
        .map_aster_err_ctx(
            "connection test cleanup failed",
            AsterError::storage_driver_error,
        )?;
    Ok(())
}

pub fn unsupported_multipart_error(policy: &storage_policy::Model) -> AsterError {
    crate::errors::storage_driver_error(
        StorageErrorKind::Unsupported,
        format!(
            "storage policy {} (driver: {:?}) does not support multipart upload",
            policy.id, policy.connector_id
        ),
    )
}
