use super::*;
use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::validation_error_with_code;
use aster_drive_storage::StorageConnectorActionSchema;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionId,
    StorageConnectorActionOutputFieldDescriptor, StorageConnectorActionOutputValueKind as Kind,
    StorageConnectorCustomActionDescriptorInput, custom_action_descriptor,
};

const ACTION_ID: &str = "configure_qiniu_cors";
aster_drive_storage::storage_connector_action_schema! {
    struct ConfigureQiniuCorsInput {}
}

pub(super) fn action_descriptor() -> StorageConnectorActionDescriptor {
    custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
        action_id: StorageConnectorActionId::declared(ACTION_ID),
        label_key: "qiniu_cors_action",
        description_key: "qiniu_cors_description",
        fields: ConfigureQiniuCorsInput::action_fields(),
        output_fields: [
            ("bucket", "qiniu_native_bucket", Kind::Text),
            ("allowed_origins", "qiniu_cors_origins", Kind::StringList),
            ("changed", "qiniu_cors_changed", Kind::Boolean),
            ("preserved_rule_count", "qiniu_cors_preserved", Kind::Number),
            ("request_id", "qiniu_cors_request_id", Kind::Text),
        ]
        .into_iter()
        .map(
            |(name, label_key, value_kind)| StorageConnectorActionOutputFieldDescriptor {
                name: name.into(),
                label_key: label_key.into(),
                value_kind,
                true_key: (value_kind == Kind::Boolean).then(|| "qiniu_cors_yes".into()),
                false_key: (value_kind == Kind::Boolean).then(|| "qiniu_cors_no".into()),
            },
        )
        .collect(),
        supports_draft: true,
        supports_saved: true,
        requires_authorization: false,
        mutates_remote_state: true,
        requires_confirmation: true,
    })
}

pub(super) fn validate_action(
    id: &StorageConnectorActionId,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    if id.as_str() != ACTION_ID {
        return Err(super::super::common::unsupported_connector_action_error(
            &QiniuConnector::descriptor_definition(),
            id,
        ));
    }
    let _: ConfigureQiniuCorsInput =
        super::super::common::decode_normalized_connector_action_input(
            &action_descriptor(),
            values,
        )?;
    Ok(())
}

pub(super) fn allowed_origins(runtime: &crate::config::RuntimeConfig) -> Result<Vec<String>> {
    let required = || {
        validation_error_with_code(
            ApiErrorCode::PolicyActionParameterRequired,
            "Configure Public site URL in system settings before configuring Kodo CORS",
        )
    };
    let value = site_url::public_site_url_config_value(runtime).ok_or_else(required)?;
    let origins = site_url::parse_public_site_url_value(&value)?;
    if origins.is_empty() {
        return Err(required());
    }
    Ok(origins)
}

#[cfg(test)]
mod tests;
