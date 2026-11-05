use super::*;
use crate::storage::connectors::{
    StorageConnectorContext, StorageConnectorCredentialInput, test_support,
};
use serde_json::json;

fn runtime(value: &str) -> crate::config::RuntimeConfig {
    let runtime = crate::config::RuntimeConfig::new();
    runtime.apply(aster_forge_db::system_config::Model {
        id: 1,
        key: site_url::PUBLIC_SITE_URL_KEY.into(),
        value: value.into(),
        value_type: aster_forge_config::ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: aster_forge_config::ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: String::new(),
        description: String::new(),
        updated_at: chrono::Utc::now(),
        updated_by: None,
    });
    runtime
}

fn config_values(bucket: &str) -> serde_json::Value {
    json!({"bucket":bucket,"base_path":"tenant","download_origin":"https://origin.example.test","download_delivery":"origin","provider_resumable_upload_strategy":"server_relay","object_storage_download_strategy":"presigned"})
}

fn connection(
    bucket: &str,
    credentials: serde_json::Value,
) -> crate::storage::StoragePolicyConnectionInput {
    crate::storage::StoragePolicyConnectionInput {
        storage: crate::storage::StorageConnectionInput {
            connector_config: test_support::connection_config(
                QiniuConnector::ID,
                2,
                config_values(bucket),
            ),
            credential: StorageConnectorCredentialInput::Static(credentials),
        },
        behavior: Default::default(),
    }
}

#[test]
fn action_declares_existing_generic_confirmation_and_localized_output_without_schema_changes() {
    let descriptor = QiniuConnector::descriptor_definition();
    descriptor.validate().unwrap();
    let action = descriptor
        .actions
        .iter()
        .find(|a| a.action_id.as_str() == ACTION_ID)
        .unwrap();
    assert!(action.requires_confirmation && action.mutates_remote_state);
    assert!(!action.requires_saved_policy);
    assert!(action.fields.is_empty());
    assert_eq!(action.endpoints.len(), 2);
    assert!(
        action
            .output_fields
            .iter()
            .any(|field| field.name == "bucket")
    );
    QiniuConnector.localization().unwrap();
    validate_action(
        &StorageConnectorActionId::declared(ACTION_ID),
        &Default::default(),
    )
    .unwrap();
    assert!(
        validate_action(
            &StorageConnectorActionId::declared("unknown"),
            &Default::default()
        )
        .is_err()
    );
    assert!(
        validate_action(
            &StorageConnectorActionId::declared(ACTION_ID),
            &std::collections::BTreeMap::from([("unexpected".into(), json!(true))])
        )
        .is_err()
    );
}

#[test]
fn origins_follow_cos_canonicalization_and_entire_invalid_config_fails_closed() {
    assert_eq!(allowed_origins(&runtime(r#"["https://DRIVE.test:443","https://drive.test","http://drive.test:80","https://[::1]:8443"]"#)).unwrap(), vec!["https://drive.test","http://drive.test","https://[::1]:8443"]);
    for value in [
        "",
        "[]",
        "not-json",
        r#"["*"]"#,
        r#"["https://good.test","https://bad.test/path"]"#,
    ] {
        assert!(allowed_origins(&runtime(value)).is_err());
    }
    assert!(allowed_origins(&crate::config::RuntimeConfig::new()).is_err());
}

#[tokio::test]
async fn draft_and_saved_actions_fail_before_provider_io_for_bad_values_or_missing_origins() {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    let config = crate::config::Config::default();
    let registry = crate::storage::DriverRegistry::noop().unwrap();
    let runtime = crate::config::RuntimeConfig::new();
    let context = StorageConnectorContext::new(&db, &config, &runtime, &registry, None);
    let policy = test_support::policy(
        QiniuConnector::ID,
        2,
        config_values("archive"),
        Default::default(),
    );
    for action_id in [ACTION_ID, "unknown"] {
        let result = QiniuConnector
            .execute_draft_action(
                &context,
                crate::storage::ExecuteDraftStorageConnectorActionInput {
                    action_id: StorageConnectorActionId::declared(action_id),
                    values: Default::default(),
                    policy_id: None,
                    connection: connection("archive", json!({})),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            result.api_error_code(),
            if action_id == ACTION_ID {
                ApiErrorCode::PolicyActionParameterRequired
            } else {
                ApiErrorCode::PolicyActionUnsupported
            }
        );
        let result = QiniuConnector
            .execute_saved_action(
                &context,
                &policy,
                crate::storage::ExecuteSavedStorageConnectorActionInput {
                    action_id: StorageConnectorActionId::declared(action_id),
                    values: Default::default(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            result.api_error_code(),
            if action_id == ACTION_ID {
                ApiErrorCode::PolicyActionParameterRequired
            } else {
                ApiErrorCode::PolicyActionUnsupported
            }
        );
    }
}

#[tokio::test]
async fn edited_draft_credentials_keep_saved_secret_accept_rotation_and_reject_other_connectors() {
    use sea_orm::ActiveModelTrait;
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    test_support::migrate_current_storage_test_schema(&db).await;
    let mut config = crate::config::Config::default();
    config.auth.storage_credential_secret_key = "qiniu-cors-tests-encryption-master-key-32".into();
    let registry = crate::storage::DriverRegistry::noop().unwrap();
    let runtime = runtime(r#"["https://drive.test"]"#);
    let context = StorageConnectorContext::new(&db, &config, &runtime, &registry, None);
    let policy = test_support::insertable_policy(test_support::policy(
        QiniuConnector::ID,
        2,
        config_values("old-bucket"),
        Default::default(),
    ))
    .insert(&db)
    .await
    .unwrap();
    crate::storage::connectors::persist_connector_credential_payload(
        &db,
        &config.auth.storage_credential_secret_key,
        policy.id,
        &aster_drive_storage::ConnectorId::declared(QiniuConnector::ID),
        1,
        &json!({"qiniu_access_key":"SAVED_AK","qiniu_secret_key":"SAVED_SK"}),
    )
    .await
    .unwrap();
    for (input, expected) in [
        (
            json!({}),
            json!({"qiniu_access_key":"SAVED_AK","qiniu_secret_key":"SAVED_SK"}),
        ),
        (
            json!({"qiniu_access_key":"","qiniu_secret_key":""}),
            json!({"qiniu_access_key":"SAVED_AK","qiniu_secret_key":"SAVED_SK"}),
        ),
        (
            json!({"qiniu_access_key":"NEW_AK","qiniu_secret_key":"NEW_SK"}),
            json!({"qiniu_access_key":"NEW_AK","qiniu_secret_key":"NEW_SK"}),
        ),
    ] {
        let draft = connection("new-bucket", input);
        let credentials = QiniuConnector
            .prepare_draft_credential(&context, Some(policy.id), draft.storage.credential)
            .await
            .unwrap();
        QiniuConnector
            .validate_credential_input(&credentials)
            .unwrap();
        let StorageConnectorCredentialInput::Static(actual) = credentials else {
            panic!("static credentials")
        };
        assert_eq!(actual, expected);
        // Restoring credentials must not restore the saved physical target.
        let normalized = QiniuConnector
            .validate_connector_config(&draft.storage.connector_config)
            .unwrap();
        assert_eq!(normalized.values["bucket"], "new-bucket");
    }
    let other = test_support::insertable_policy(test_support::local_policy("data"))
        .insert(&db)
        .await
        .unwrap();
    assert!(
        QiniuConnector
            .prepare_draft_credential(
                &context,
                Some(other.id),
                StorageConnectorCredentialInput::Static(json!({}))
            )
            .await
            .is_err()
    );
    assert!(
        QiniuConnector
            .validate_credential_input(&StorageConnectorCredentialInput::Static(json!({})))
            .is_err()
    );
}
