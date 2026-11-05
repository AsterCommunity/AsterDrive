use super::*;
use crate::api::api_error_code::ApiErrorCode;
use crate::config::site_url;
use crate::errors::validation_error_with_code;
use crate::storage::connectors::common::{action_output_field, boolean_action_output_field};
use aster_drive_storage::StorageConnectorActionSchema;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorActionDescriptor, StorageConnectorActionId,
    StorageConnectorActionOutputValueKind as Kind, StorageConnectorCustomActionDescriptorInput,
    custom_action_descriptor,
};

const ACTION_ID: &str = "configure_qiniu_cors";
aster_drive_storage::storage_connector_action_schema! {
    struct ConfigureQiniuCorsInput {}
}

pub(super) fn configure_cors_action_descriptor() -> StorageConnectorActionDescriptor {
    custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
        action_id: StorageConnectorActionId::declared(ACTION_ID),
        label_key: "qiniu_cors_action",
        description_key: "qiniu_cors_description",
        fields: ConfigureQiniuCorsInput::action_fields(),
        output_fields: vec![
            action_output_field("bucket", "qiniu_native_bucket", Kind::Text),
            action_output_field("allowed_origins", "qiniu_cors_origins", Kind::StringList),
            boolean_action_output_field(
                "changed",
                "qiniu_cors_changed",
                "qiniu_cors_yes",
                "qiniu_cors_no",
            ),
            action_output_field("preserved_rule_count", "qiniu_cors_preserved", Kind::Number),
            action_output_field("request_id", "qiniu_cors_request_id", Kind::Text),
        ],
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
            &configure_cors_action_descriptor(),
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
