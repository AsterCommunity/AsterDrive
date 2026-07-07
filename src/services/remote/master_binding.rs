//! 服务模块：`remote::master_binding`。

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::master_binding_repo;
use crate::entities::master_binding;
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::services::remote::storage_target;
use crate::storage::StorageDriver;
use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_NONCE_TTL_SECS,
    INTERNAL_AUTH_SIGNATURE_HEADER, INTERNAL_AUTH_SKEW_SECS, INTERNAL_AUTH_TIMESTAMP_HEADER,
    PRESIGNED_AUTH_ACCESS_KEY_QUERY, PRESIGNED_AUTH_EXPIRES_QUERY, PRESIGNED_AUTH_SIGNATURE_QUERY,
    REMOTE_POLICY_MAX_FILE_SIZE_QUERY, REMOTE_STORAGE_TARGET_KEY_QUERY, normalize_remote_base_url,
    sign_presigned_request,
};
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use sea_orm::{ConnectionTrait, Set};
use sha2::Sha256;
use std::sync::Arc;

const STORAGE_NAMESPACE_ALLOCATION_ATTEMPTS: usize = 8;

#[derive(Clone)]
pub struct AuthorizedMasterBinding {
    pub binding: master_binding::Model,
    pub ingress_driver: Arc<dyn StorageDriver>,
    pub ingress_max_file_size: i64,
}

#[derive(Debug, Clone)]
pub struct UpsertMasterBindingInput {
    pub name: String,
    pub master_url: String,
    pub access_key: String,
    pub secret_key: String,
    pub is_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SyncMasterBindingInput {
    pub name: String,
    pub is_enabled: bool,
}

pub async fn upsert_from_enrollment<C: ConnectionTrait>(
    db: &C,
    input: UpsertMasterBindingInput,
) -> Result<(master_binding::Model, &'static str)> {
    let normalized = normalize_upsert_input(input)?;
    let now = Utc::now();

    match master_binding_repo::find_by_access_key(db, &normalized.access_key).await? {
        Some(existing) => update_existing_from_enrollment(db, existing, &normalized, now).await,
        None => {
            let mut candidates = Vec::with_capacity(STORAGE_NAMESPACE_ALLOCATION_ATTEMPTS);
            for _ in 0..STORAGE_NAMESPACE_ALLOCATION_ATTEMPTS {
                let storage_namespace = new_storage_namespace();
                let created = master_binding_repo::create_ignoring_storage_namespace_conflict(
                    db,
                    master_binding::ActiveModel {
                        name: Set(normalized.name.clone()),
                        master_url: Set(normalized.master_url.clone()),
                        access_key: Set(normalized.access_key.clone()),
                        secret_key: Set(normalized.secret_key.clone()),
                        storage_namespace: Set(storage_namespace.clone()),
                        is_enabled: Set(normalized.is_enabled),
                        created_at: Set(now),
                        updated_at: Set(now),
                        ..Default::default()
                    },
                )
                .await?;

                if let Some(created) = created {
                    return Ok((created, "created"));
                }

                if let Some(existing) =
                    master_binding_repo::find_by_access_key(db, &normalized.access_key).await?
                {
                    return update_existing_from_enrollment(db, existing, &normalized, now).await;
                }

                candidates.push(storage_namespace);
            }

            tracing::error!(
                attempts = STORAGE_NAMESPACE_ALLOCATION_ATTEMPTS,
                candidates = ?candidates,
                "failed to allocate unique master binding storage namespace after crate::utils::id::new_short_token candidates conflicted during insert"
            );
            Err(AsterError::internal_error(
                "failed to allocate unique master binding storage namespace",
            ))
        }
    }
}

async fn update_existing_from_enrollment<C: ConnectionTrait>(
    db: &C,
    existing: master_binding::Model,
    normalized: &UpsertMasterBindingInput,
    now: chrono::DateTime<Utc>,
) -> Result<(master_binding::Model, &'static str)> {
    if existing.secret_key != normalized.secret_key {
        return Err(AsterError::validation_error(
            "master binding access_key already exists with different credentials",
        ));
    }
    let mut active: master_binding::ActiveModel = existing.into();
    active.name = Set(normalized.name.clone());
    active.master_url = Set(normalized.master_url.clone());
    active.secret_key = Set(normalized.secret_key.clone());
    active.is_enabled = Set(normalized.is_enabled);
    active.updated_at = Set(now);
    let updated = master_binding_repo::update(db, active).await?;
    Ok((updated, "updated"))
}

pub async fn authorize_internal_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<AuthorizedMasterBinding> {
    let binding = authorize_binding_request(state, req, false).await?;
    resolve_authorized_ingress(state, binding, remote_storage_target_key(req)?, 0).await
}

pub async fn authorize_internal_write_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<AuthorizedMasterBinding> {
    let binding = authorize_binding_request(state, req, false).await?;
    resolve_authorized_ingress(
        state,
        binding,
        remote_storage_target_key(req)?,
        required_policy_max_file_size(req)?,
    )
    .await
}

pub async fn authorize_internal_binding_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<master_binding::Model> {
    authorize_binding_request(state, req, false).await
}

pub async fn authorize_binding_sync_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<master_binding::Model> {
    authorize_binding_request(state, req, true).await
}

pub async fn authorize_presigned_put_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<AuthorizedMasterBinding> {
    if req.method() != actix_web::http::Method::PUT {
        return Err(AsterError::auth_token_invalid(
            "remote presigned auth only supports PUT",
        ));
    }

    let binding = authorize_presigned_binding_request(state, req).await?;
    resolve_authorized_ingress(
        state,
        binding,
        remote_storage_target_key(req)?,
        required_policy_max_file_size(req)?,
    )
    .await
}

pub async fn authorize_presigned_get_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<AuthorizedMasterBinding> {
    if req.method() != actix_web::http::Method::GET {
        return Err(AsterError::auth_token_invalid(
            "remote presigned auth only supports GET",
        ));
    }

    let binding = authorize_presigned_binding_request(state, req).await?;
    resolve_authorized_ingress(state, binding, remote_storage_target_key(req)?, 0).await
}

pub async fn sync_from_primary<S: FollowerRuntimeState>(
    state: &S,
    access_key: &str,
    input: SyncMasterBindingInput,
) -> Result<master_binding::Model> {
    let existing = master_binding_repo::find_by_access_key(state.writer_db(), access_key)
        .await?
        .ok_or_else(|| AsterError::auth_invalid_credentials("unknown internal access_key"))?;
    let normalized = normalize_sync_input(input)?;

    let mut active: master_binding::ActiveModel = existing.into();
    active.name = Set(normalized.name);
    active.is_enabled = Set(normalized.is_enabled);
    active.updated_at = Set(Utc::now());

    let updated = master_binding_repo::update(state.writer_db(), active).await?;
    state
        .driver_registry()
        .reload_master_bindings(state.writer_db())
        .await?;
    Ok(updated)
}

async fn authorize_binding_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
    allow_disabled: bool,
) -> Result<master_binding::Model> {
    let access_key = header_value(req, INTERNAL_AUTH_ACCESS_KEY_HEADER)?;
    let timestamp = header_value(req, INTERNAL_AUTH_TIMESTAMP_HEADER)?
        .parse::<i64>()
        .map_err(|_| AsterError::auth_token_invalid("invalid internal auth timestamp"))?;
    let nonce = header_value(req, INTERNAL_AUTH_NONCE_HEADER)?;
    let signature = header_value(req, INTERNAL_AUTH_SIGNATURE_HEADER)?;

    let now = Utc::now().timestamp();
    if (now - timestamp).abs() > INTERNAL_AUTH_SKEW_SECS {
        return Err(AsterError::auth_token_invalid(
            "internal auth timestamp is outside allowed skew",
        ));
    }

    let nonce_cache_key = format!("internal_remote_nonce:{access_key}:{nonce}");
    let binding = state
        .driver_registry()
        .find_master_binding_by_access_key(&access_key)
        .ok_or_else(|| AsterError::auth_invalid_credentials("unknown internal access_key"))?;
    if !allow_disabled && !binding.is_enabled {
        return Err(precondition_failed_with_code(
            ApiErrorCode::MasterBindingDisabled,
            "master binding is disabled",
        ));
    }

    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_else(|| req.uri().path());
    let content_length = req
        .headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    if !verify_signature(
        &binding.secret_key,
        req.method().as_str(),
        path_and_query,
        timestamp,
        &nonce,
        content_length,
        &signature,
    ) {
        return Err(AsterError::auth_invalid_credentials(
            "internal auth signature mismatch",
        ));
    }

    if !state
        .cache()
        .set_bytes_if_absent(
            &nonce_cache_key,
            Vec::new(),
            Some(INTERNAL_AUTH_NONCE_TTL_SECS),
        )
        .await
    {
        return Err(AsterError::auth_token_invalid(
            "internal auth nonce has already been used",
        ));
    }

    Ok(binding)
}

async fn authorize_presigned_binding_request<S: FollowerRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
) -> Result<master_binding::Model> {
    let access_key = query_value(req, PRESIGNED_AUTH_ACCESS_KEY_QUERY)?;
    let expires_at = query_value(req, PRESIGNED_AUTH_EXPIRES_QUERY)?
        .parse::<i64>()
        .map_err(|_| AsterError::auth_token_invalid("invalid remote presigned expiry"))?;
    let signature = query_value(req, PRESIGNED_AUTH_SIGNATURE_QUERY)?;

    if Utc::now().timestamp() > expires_at {
        return Err(AsterError::auth_token_invalid(
            "remote presigned URL has expired",
        ));
    }

    let binding = state
        .driver_registry()
        .find_master_binding_by_access_key(&access_key)
        .ok_or_else(|| AsterError::auth_invalid_credentials("unknown internal access_key"))?;
    if !binding.is_enabled {
        return Err(precondition_failed_with_code(
            ApiErrorCode::MasterBindingDisabled,
            "master binding is disabled",
        ));
    }

    if !verify_presigned_signature(
        &binding.secret_key,
        req.method().as_str(),
        &presigned_request_target(req),
        &access_key,
        expires_at,
        &signature,
    ) {
        return Err(AsterError::auth_invalid_credentials(
            "remote presigned signature mismatch",
        ));
    }

    Ok(binding)
}

pub fn provider_storage_path(binding: &master_binding::Model, object_key: &str) -> Result<String> {
    let object_key = crate::storage::object_key::normalize_relative_key(object_key)?;
    if object_key == "." || object_key.is_empty() {
        return Err(AsterError::validation_error(
            "object key cannot target the storage namespace root",
        ));
    }
    Ok(crate::storage::object_key::join_key_prefix(
        &binding.storage_namespace,
        &object_key,
    ))
}

pub fn provider_storage_prefix(binding: &master_binding::Model, prefix: &str) -> Result<String> {
    let prefix = crate::storage::object_key::normalize_relative_key(prefix)?;
    if prefix == "." || prefix.is_empty() {
        Ok(binding.storage_namespace.clone())
    } else {
        Ok(crate::storage::object_key::join_key_prefix(
            &binding.storage_namespace,
            &prefix,
        ))
    }
}

pub async fn assert_follower_ready<S: FollowerRuntimeState>(state: &S) -> Result<()> {
    let bindings = master_binding_repo::find_all(state.writer_db()).await?;
    let enabled_bindings: Vec<_> = bindings
        .into_iter()
        .filter(|binding| binding.is_enabled)
        .collect();
    if enabled_bindings.is_empty() {
        return Err(AsterError::storage_driver_error(
            "no active master bindings configured",
        ));
    }

    for binding in enabled_bindings {
        let _ = storage_target::resolve_effective_target(state, &binding).await?;
    }
    Ok(())
}

async fn resolve_authorized_ingress<S: FollowerRuntimeState>(
    state: &S,
    binding: master_binding::Model,
    target_key: Option<String>,
    policy_max_file_size: i64,
) -> Result<AuthorizedMasterBinding> {
    let target = match target_key.as_deref() {
        Some(target_key) => {
            storage_target::resolve_target_by_key(state, &binding, target_key).await?
        }
        // Compatibility only: policies created before remote_storage_target_key
        // existed do not send target_key in signed internal/presigned requests.
        // Keep them on the binding default target instead of guessing a target
        // from primary-side state.
        None => storage_target::resolve_effective_target(state, &binding).await?,
    };

    Ok(AuthorizedMasterBinding {
        binding,
        ingress_driver: target.driver,
        ingress_max_file_size: policy_max_file_size,
    })
}

fn normalize_upsert_input(input: UpsertMasterBindingInput) -> Result<UpsertMasterBindingInput> {
    Ok(UpsertMasterBindingInput {
        name: normalize_non_blank("name", &input.name)?,
        master_url: normalize_remote_base_url(&input.master_url)?,
        access_key: normalize_non_blank("access_key", &input.access_key)?,
        secret_key: normalize_non_blank("secret_key", &input.secret_key)?,
        is_enabled: input.is_enabled,
    })
}

fn normalize_sync_input(input: SyncMasterBindingInput) -> Result<SyncMasterBindingInput> {
    Ok(SyncMasterBindingInput {
        name: normalize_non_blank("name", &input.name)?,
        is_enabled: input.is_enabled,
    })
}

fn verify_signature(
    secret_key: &str,
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    content_length: Option<u64>,
    provided_signature: &str,
) -> bool {
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(provided_signature, &mut decoded).is_err() {
        return false;
    }

    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}",
        method,
        path_and_query,
        timestamp,
        nonce,
        content_length
            .map(|value| value.to_string())
            .unwrap_or_default()
    );
    let Ok(mut mac) = <Hmac<Sha256> as KeyInit>::new_from_slice(secret_key.as_bytes()) else {
        return false;
    };
    mac.update(canonical.as_bytes());
    mac.verify_slice(&decoded).is_ok()
}

fn verify_presigned_signature(
    secret_key: &str,
    method: &str,
    request_target: &str,
    access_key: &str,
    expires_at: i64,
    provided_signature: &str,
) -> bool {
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(provided_signature, &mut decoded).is_err() {
        return false;
    }

    let expected =
        sign_presigned_request(secret_key, method, request_target, access_key, expires_at);
    let Ok(expected) = hex::decode(expected) else {
        return false;
    };
    expected == decoded
}

fn presigned_request_target(req: &actix_web::HttpRequest) -> String {
    let path = req.uri().path();
    let Some(query) = req.uri().query() else {
        return path.to_string();
    };

    let filtered: Vec<&str> = query
        .split('&')
        .filter(|segment| !segment.is_empty())
        .filter(|segment| {
            let key = segment.split('=').next().unwrap_or_default();
            key != PRESIGNED_AUTH_ACCESS_KEY_QUERY
                && key != PRESIGNED_AUTH_EXPIRES_QUERY
                && key != PRESIGNED_AUTH_SIGNATURE_QUERY
        })
        .collect();

    if filtered.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", filtered.join("&"))
    }
}

fn header_value(req: &actix_web::HttpRequest, name: &str) -> Result<String> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AsterError::auth_token_invalid(format!("missing header {name}")))
}

fn query_value(req: &actix_web::HttpRequest, name: &str) -> Result<String> {
    actix_web::web::Query::<std::collections::HashMap<String, String>>::from_query(
        req.query_string(),
    )
    .map_err(|_| AsterError::auth_token_invalid("invalid query string"))?
    .get(name)
    .cloned()
    .filter(|value| !value.is_empty())
    .ok_or_else(|| AsterError::auth_token_invalid(format!("missing query parameter '{name}'")))
}

fn optional_query_value(req: &actix_web::HttpRequest, name: &str) -> Result<Option<String>> {
    Ok(
        actix_web::web::Query::<std::collections::HashMap<String, String>>::from_query(
            req.query_string(),
        )
        .map_err(|_| AsterError::auth_token_invalid("invalid query string"))?
        .get(name)
        .cloned()
        .filter(|value| !value.trim().is_empty()),
    )
}

fn remote_storage_target_key(req: &actix_web::HttpRequest) -> Result<Option<String>> {
    optional_query_value(req, REMOTE_STORAGE_TARGET_KEY_QUERY)
}

fn required_policy_max_file_size(req: &actix_web::HttpRequest) -> Result<i64> {
    let Some(value) = optional_query_value(req, REMOTE_POLICY_MAX_FILE_SIZE_QUERY)? else {
        return Err(AsterError::auth_token_invalid(
            "missing max_file_size query value",
        ));
    };
    let parsed = value
        .parse::<i64>()
        .map_err(|_| AsterError::auth_token_invalid("invalid max_file_size query value"))?;
    if parsed < 0 {
        return Err(AsterError::auth_token_invalid(
            "max_file_size query value cannot be negative",
        ));
    }
    Ok(parsed)
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

fn new_storage_namespace() -> String {
    format!("mb_{}", crate::utils::id::new_short_token())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    fn binding() -> master_binding::Model {
        let now = Utc::now();
        master_binding::Model {
            id: 1,
            name: "binding".to_string(),
            master_url: "https://master.example.com".to_string(),
            access_key: "ak".to_string(),
            secret_key: "sk".to_string(),
            storage_namespace: "mb_test".to_string(),
            is_enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn provider_storage_path_scopes_safe_keys() {
        assert_eq!(
            provider_storage_path(&binding(), "folder/file.txt").unwrap(),
            "mb_test/folder/file.txt"
        );
    }

    #[test]
    fn provider_storage_path_rejects_namespace_root() {
        assert!(provider_storage_path(&binding(), "").is_err());
        assert!(provider_storage_path(&binding(), ".").is_err());
        assert!(provider_storage_path(&binding(), "/").is_err());
    }

    #[test]
    fn provider_storage_path_rejects_escape_attempts() {
        assert!(provider_storage_path(&binding(), "../secret.txt").is_err());
        assert!(provider_storage_path(&binding(), "folder\\..\\secret.txt").is_err());
    }

    #[test]
    fn provider_storage_prefix_allows_namespace_root() {
        assert_eq!(provider_storage_prefix(&binding(), "").unwrap(), "mb_test");
        assert_eq!(provider_storage_prefix(&binding(), ".").unwrap(), "mb_test");
        assert_eq!(provider_storage_prefix(&binding(), "/").unwrap(), "mb_test");
        assert_eq!(
            provider_storage_prefix(&binding(), "folder").unwrap(),
            "mb_test/folder"
        );
    }

    #[test]
    fn policy_max_file_size_parses_signed_query_boundary_values() {
        let positive =
            TestRequest::with_uri("/api/v1/internal/storage/objects/file.bin?max_file_size=4096")
                .to_http_request();
        assert_eq!(required_policy_max_file_size(&positive).unwrap(), 4096);

        let zero =
            TestRequest::with_uri("/api/v1/internal/storage/objects/file.bin?max_file_size=0")
                .to_http_request();
        assert_eq!(required_policy_max_file_size(&zero).unwrap(), 0);
    }

    #[test]
    fn policy_max_file_size_rejects_invalid_signed_query_values() {
        let missing =
            TestRequest::with_uri("/api/v1/internal/storage/objects/file.bin").to_http_request();
        assert!(required_policy_max_file_size(&missing).is_err());

        let empty =
            TestRequest::with_uri("/api/v1/internal/storage/objects/file.bin?max_file_size=%20")
                .to_http_request();
        assert!(required_policy_max_file_size(&empty).is_err());

        let invalid =
            TestRequest::with_uri("/api/v1/internal/storage/objects/file.bin?max_file_size=abc")
                .to_http_request();
        assert!(required_policy_max_file_size(&invalid).is_err());

        let negative =
            TestRequest::with_uri("/api/v1/internal/storage/objects/file.bin?max_file_size=-1")
                .to_http_request();
        assert!(required_policy_max_file_size(&negative).is_err());
    }
}
