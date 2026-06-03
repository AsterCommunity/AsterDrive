//! Google Drive storage policy OAuth service.

use base64::Engine as _;
use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::subcode::ApiSubcode;
use crate::config::site_url;
use crate::db::repository::{google_drive_oauth_flow_repo, policy_repo};
use crate::entities::google_drive_oauth_flow;
use crate::errors::{
    AsterError, MapAsterErr, Result, auth_invalid_credentials_with_subcode,
    validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::storage::drivers::google_drive::{
    encrypt_refresh_token, google_drive_parent_id, google_drive_scopes,
};
use crate::types::{
    DriverType, StoragePolicyOptions, parse_storage_policy_options,
    serialize_storage_policy_options,
};
use crate::utils::numbers::u64_to_i64;
use crate::utils::{OUTBOUND_HTTP_USER_AGENT, hash, id};

const AUTHORIZATION_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const FLOW_TTL_SECS: u64 = 300;
const HTTP_TIMEOUT_SECS: u64 = 20;
const DEFAULT_RETURN_PATH: &str = "/admin/policies";
const CALLBACK_PATH: &str = "/api/v1/admin/policies/google-drive/oauth/callback";

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct GoogleDriveStartPolicyAuthRequest {
    pub return_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct GoogleDriveStartPolicyAuthResponse {
    pub authorization_url: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct GoogleDrivePolicyAuthStatus {
    pub policy_id: i64,
    pub authorized: bool,
    pub account_email: Option<String>,
    pub account_name: Option<String>,
    pub account_id: Option<String>,
    pub token_status: Option<String>,
    pub root: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, ToSchema)
)]
pub struct GoogleDrivePolicyAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GoogleDrivePolicyAuthCallbackResult {
    pub return_path: String,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveErrorResponse {
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveUserInfo {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

struct GoogleDrivePolicyAuthSettings {
    client_id: String,
    client_secret: String,
    scopes: String,
}

pub async fn get_policy_auth_status(
    state: &PrimaryAppState,
    policy_id: i64,
) -> Result<GoogleDrivePolicyAuthStatus> {
    let policy = policy_repo::find_by_id(state.reader_db(), policy_id).await?;
    ensure_google_drive_policy(&policy)?;
    let options = parse_storage_policy_options(policy.options.as_ref());
    let root = google_drive_parent_id(&options);
    Ok(GoogleDrivePolicyAuthStatus {
        policy_id,
        authorized: options.google_drive_authorized(),
        account_email: options.google_drive_account_email,
        account_name: options.google_drive_account_name,
        account_id: options.google_drive_account_id,
        token_status: options.google_drive_token_status,
        root,
        last_error: options.google_drive_last_error,
    })
}

pub async fn cleanup_expired_flows(state: &PrimaryAppState) -> Result<u64> {
    google_drive_oauth_flow_repo::cleanup_expired(state.writer_db(), Utc::now()).await
}

pub async fn start_policy_auth(
    state: &PrimaryAppState,
    req: &actix_web::HttpRequest,
    policy_id: i64,
    return_path: Option<&str>,
) -> Result<GoogleDriveStartPolicyAuthResponse> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    ensure_google_drive_policy(&policy)?;
    let options = parse_storage_policy_options(policy.options.as_ref());
    let settings = policy_auth_settings(&policy, &options)?;
    let return_path = normalize_return_path(return_path)?;
    let redirect_uri = callback_redirect_uri(state, req)?;
    let state_value = format!("gdrive_policy_{}", id::new_short_token());
    let pkce_verifier = build_pkce_verifier();
    let authorization_url =
        build_authorization_url(&settings, &redirect_uri, &state_value, &pkce_verifier)?;
    let now = Utc::now();
    let ttl = u64_to_i64(FLOW_TTL_SECS, "Google Drive OAuth flow ttl")?;
    let flow = google_drive_oauth_flow::ActiveModel {
        policy_id: Set(policy_id),
        state_hash: Set(state_hash(&state_value)),
        pkce_verifier: Set(pkce_verifier),
        redirect_uri: Set(redirect_uri),
        return_path: Set(Some(return_path)),
        created_at: Set(now),
        expires_at: Set(now + Duration::seconds(ttl)),
        consumed_at: Set(None),
        ..Default::default()
    };
    google_drive_oauth_flow_repo::create(state.writer_db(), flow).await?;

    Ok(GoogleDriveStartPolicyAuthResponse { authorization_url })
}

pub async fn finish_policy_auth_callback(
    state: &PrimaryAppState,
    query: &GoogleDrivePolicyAuthCallbackQuery,
) -> Result<GoogleDrivePolicyAuthCallbackResult> {
    if let Some(error) = query.error.as_deref() {
        let description = query
            .error_description
            .as_deref()
            .map(sanitize_error_fragment)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| sanitize_error_fragment(error));
        return Err(auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            format!("Google Drive authorization failed: {description}"),
        ));
    }
    let code = query.code.as_deref().ok_or_else(|| {
        auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            "Google Drive callback missing code",
        )
    })?;
    let state_value = query.state.as_deref().ok_or_else(|| {
        auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            "Google Drive callback missing state",
        )
    })?;

    let now = Utc::now();
    let flow = google_drive_oauth_flow_repo::consume_by_state_hash(
        state.writer_db(),
        &state_hash(state_value),
        now,
    )
    .await?
    .ok_or_else(|| {
        auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            "Google Drive authorization state is invalid or expired",
        )
    })?;

    let policy = policy_repo::find_by_id(state.writer_db(), flow.policy_id).await?;
    ensure_google_drive_policy(&policy)?;
    let mut options = parse_storage_policy_options(policy.options.as_ref());
    let settings = policy_auth_settings(&policy, &options)?;
    let http_client = google_drive_http_client()?;
    let token = exchange_code_for_token(
        &http_client,
        &settings,
        code,
        &flow.redirect_uri,
        &flow.pkce_verifier,
    )
    .await?;
    let refresh_token = token.refresh_token.as_deref().ok_or_else(|| {
        auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            "Google Drive did not return a refresh token",
        )
    })?;
    let user = fetch_userinfo(&http_client, &token.access_token).await?;

    options.google_drive_refresh_token = Some(encrypt_refresh_token(policy.id, refresh_token)?);
    options.google_drive_account_id = normalize_optional_snapshot(user.id.as_deref());
    options.google_drive_account_email = normalize_optional_snapshot(user.email.as_deref());
    options.google_drive_account_name = normalize_optional_snapshot(user.name.as_deref());
    options.google_drive_token_status = Some("authorized".to_string());
    options.google_drive_last_error = None;

    let mut active = policy.into_active_model();
    active.options = Set(serialize_storage_policy_options(&options).map_err(|error| {
        AsterError::internal_error(format!("serialize Google Drive policy options: {error}"))
    })?);
    active.updated_at = Set(now);
    active
        .update(state.writer_db())
        .await
        .map_aster_err(AsterError::database_operation)?;

    state.driver_registry.invalidate(flow.policy_id);
    state.policy_snapshot.reload(state.writer_db()).await?;

    Ok(GoogleDrivePolicyAuthCallbackResult {
        return_path: flow
            .return_path
            .unwrap_or_else(|| DEFAULT_RETURN_PATH.to_string()),
    })
}

fn ensure_google_drive_policy(policy: &crate::entities::storage_policy::Model) -> Result<()> {
    if policy.driver_type != DriverType::GoogleDrive {
        return Err(AsterError::validation_error(format!(
            "storage policy #{} is not a Google Drive policy",
            policy.id
        )));
    }
    Ok(())
}

fn policy_auth_settings(
    policy: &crate::entities::storage_policy::Model,
    options: &StoragePolicyOptions,
) -> Result<GoogleDrivePolicyAuthSettings> {
    let client_id = policy.access_key.trim();
    if client_id.is_empty() {
        return Err(validation_error_with_subcode(
            ApiSubcode::GoogleDriveMisconfigured,
            "Google Drive client id is not configured",
        ));
    }
    let client_secret = policy.secret_key.trim();
    if client_secret.is_empty() {
        return Err(validation_error_with_subcode(
            ApiSubcode::GoogleDriveMisconfigured,
            "Google Drive client secret is not configured",
        ));
    }
    Ok(GoogleDrivePolicyAuthSettings {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        scopes: google_drive_scopes(options),
    })
}

fn build_authorization_url(
    settings: &GoogleDrivePolicyAuthSettings,
    redirect_uri: &str,
    state: &str,
    pkce_verifier: &str,
) -> Result<String> {
    let mut authorization_url = Url::parse(AUTHORIZATION_URL).map_aster_err_ctx(
        "invalid Google Drive authorization URL",
        AsterError::config_error,
    )?;
    let pkce_challenge = build_pkce_challenge(pkce_verifier);
    {
        let mut query = authorization_url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &settings.client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("scope", &settings.scopes);
        query.append_pair("state", state);
        query.append_pair("code_challenge", &pkce_challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("access_type", "offline");
        query.append_pair("include_granted_scopes", "true");
        query.append_pair("prompt", "consent");
    }
    Ok(authorization_url.to_string())
}

async fn exchange_code_for_token(
    http_client: &reqwest::Client,
    settings: &GoogleDrivePolicyAuthSettings,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
) -> Result<GoogleDriveTokenResponse> {
    let form = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer.append_pair("grant_type", "authorization_code");
        serializer.append_pair("code", code);
        serializer.append_pair("redirect_uri", redirect_uri);
        serializer.append_pair("code_verifier", pkce_verifier);
        serializer.append_pair("client_id", &settings.client_id);
        serializer.append_pair("client_secret", &settings.client_secret);
        serializer.finish()
    };
    send_token_request(http_client, form, "Google Drive token exchange").await
}

async fn send_token_request(
    http_client: &reqwest::Client,
    form: String,
    context: &str,
) -> Result<GoogleDriveTokenResponse> {
    let response = http_client
        .post(TOKEN_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .map_err(|error| {
            validation_error_with_subcode(
                ApiSubcode::GoogleDriveTransient,
                format!("{context} failed: {error}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(google_drive_endpoint_error(response, context).await);
    }
    let token = response
        .json::<GoogleDriveTokenResponse>()
        .await
        .map_err(|error| {
            validation_error_with_subcode(
                ApiSubcode::GoogleDriveTransient,
                format!("{context} response is invalid: {error}"),
            )
        })?;
    if token.access_token.trim().is_empty() {
        return Err(auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            format!("{context} response missing access_token"),
        ));
    }
    if let Some(token_type) = token.token_type.as_deref()
        && !token_type.eq_ignore_ascii_case("bearer")
    {
        return Err(auth_invalid_credentials_with_subcode(
            ApiSubcode::GoogleDriveConnectionExpired,
            format!("{context} response returned unsupported token_type"),
        ));
    }
    Ok(token)
}

async fn fetch_userinfo(
    http_client: &reqwest::Client,
    access_token: &str,
) -> Result<GoogleDriveUserInfo> {
    let response = http_client
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| {
            validation_error_with_subcode(
                ApiSubcode::GoogleDriveTransient,
                format!("Google Drive userinfo request failed: {error}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(google_drive_endpoint_error(response, "Google Drive userinfo request").await);
    }
    response
        .json::<GoogleDriveUserInfo>()
        .await
        .map_err(|error| {
            validation_error_with_subcode(
                ApiSubcode::GoogleDriveTransient,
                format!("Google Drive userinfo response is invalid: {error}"),
            )
        })
}

async fn google_drive_endpoint_error(response: reqwest::Response, context: &str) -> AsterError {
    let status = response.status();
    let provider_error = response.json::<GoogleDriveErrorResponse>().await.ok();
    let detail = provider_error
        .as_ref()
        .and_then(google_drive_error_message)
        .unwrap_or_else(|| status.to_string());
    let subcode = if google_drive_error_is_auth_failure(status, context, provider_error.as_ref()) {
        ApiSubcode::GoogleDriveConnectionExpired
    } else if status == reqwest::StatusCode::NOT_FOUND {
        ApiSubcode::GoogleDriveRemoteNotFound
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || google_drive_error_is_rate_limited(provider_error.as_ref())
    {
        ApiSubcode::GoogleDriveRateLimited
    } else if status == reqwest::StatusCode::FORBIDDEN {
        ApiSubcode::GoogleDrivePermissionDenied
    } else {
        ApiSubcode::GoogleDriveTransient
    };
    validation_error_with_subcode(subcode, format!("{context} failed: {detail}"))
}

fn google_drive_error_is_auth_failure(
    status: reqwest::StatusCode,
    context: &str,
    error: Option<&GoogleDriveErrorResponse>,
) -> bool {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return true;
    }
    if status != reqwest::StatusCode::BAD_REQUEST || !context.contains("token") {
        return false;
    }
    google_drive_error_reasons(error).any(|reason| {
        matches!(
            reason,
            "invalid_grant" | "invalid_client" | "invalid_request" | "unauthorized_client"
        )
    })
}

fn google_drive_error_is_rate_limited(error: Option<&GoogleDriveErrorResponse>) -> bool {
    google_drive_error_reasons(error).any(|reason| {
        let lower = reason.to_ascii_lowercase();
        lower.contains("ratelimit")
            || lower.contains("rate_limit")
            || lower.contains("quota")
            || lower.contains("limitexceeded")
    })
}

fn google_drive_error_reasons(
    error: Option<&GoogleDriveErrorResponse>,
) -> impl Iterator<Item = &str> {
    error
        .and_then(|error| error.error.as_ref())
        .into_iter()
        .flat_map(|error| {
            error
                .as_str()
                .into_iter()
                .chain(error.get("reason").and_then(Value::as_str))
                .chain(
                    error
                        .get("errors")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|item| item.get("reason").and_then(Value::as_str)),
                )
        })
}

fn google_drive_error_message(error: &GoogleDriveErrorResponse) -> Option<String> {
    if let Some(description) = error.error_description.as_deref() {
        return Some(sanitize_error_fragment(description));
    }
    let error = error.error.as_ref()?;
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(sanitize_error_fragment)
}

fn callback_redirect_uri(state: &PrimaryAppState, req: &actix_web::HttpRequest) -> Result<String> {
    let conn = req.connection_info();
    let uri = site_url::public_app_url_for_request(
        &state.runtime_config,
        CALLBACK_PATH,
        conn.scheme(),
        conn.host(),
    )
    .ok_or_else(|| {
        validation_error_with_subcode(
            ApiSubcode::GoogleDriveMisconfigured,
            "cannot build Google Drive callback redirect URI; configure public_site_url",
        )
    })?;
    if uri.starts_with('/') {
        return Err(validation_error_with_subcode(
            ApiSubcode::GoogleDriveMisconfigured,
            "Google Drive callback redirect URI must be absolute; configure public_site_url",
        ));
    }
    Ok(uri)
}

fn normalize_return_path(value: Option<&str>) -> Result<String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(DEFAULT_RETURN_PATH.to_string());
    };
    if !value.starts_with('/') || value.starts_with("//") || value.contains('\\') {
        return Err(AsterError::validation_error(
            "invalid Google Drive policy auth return_path",
        ));
    }
    if value.len() > 2048 {
        return Err(AsterError::validation_error(
            "Google Drive policy auth return_path is too long",
        ));
    }
    Ok(value.to_string())
}

fn normalize_optional_snapshot(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(255).collect::<String>())
}

fn google_drive_http_client() -> Result<reqwest::Client> {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build Google Drive HTTP client",
            AsterError::internal_error,
        )
}

fn state_hash(state: &str) -> String {
    hash::sha256_hex(state.as_bytes())
}

fn build_pkce_verifier() -> String {
    let mut bytes = [0_u8; 32];
    let mut rng = rand::rng();
    rng.fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn build_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn sanitize_error_fragment(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(256)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use reqwest::StatusCode;
    use serde_json::json;

    use super::{
        DEFAULT_RETURN_PATH, GoogleDriveErrorResponse, GoogleDrivePolicyAuthSettings,
        build_authorization_url, build_pkce_challenge, google_drive_error_is_auth_failure,
        google_drive_error_is_rate_limited, normalize_optional_snapshot, normalize_return_path,
        policy_auth_settings, sanitize_error_fragment, state_hash,
    };
    use crate::api::subcode::ApiSubcode;
    use crate::entities::storage_policy;
    use crate::types::{
        DriverType, StoragePolicyOptions, StoredStoragePolicyAllowedTypes,
        StoredStoragePolicyOptions, serialize_storage_policy_options,
    };

    fn provider_error(value: serde_json::Value) -> GoogleDriveErrorResponse {
        serde_json::from_value(value).expect("provider error should deserialize")
    }

    fn google_drive_policy(
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        options: StoragePolicyOptions,
    ) -> storage_policy::Model {
        let now = Utc::now();
        storage_policy::Model {
            id: 42,
            name: "Google Drive".to_string(),
            driver_type: DriverType::GoogleDrive,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            base_path: String::new(),
            remote_node_id: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: serialize_storage_policy_options(&options).expect("options should serialize"),
            is_default: false,
            chunk_size: 0,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn normalize_return_path_defaults_and_trims_site_paths() {
        assert_eq!(normalize_return_path(None).unwrap(), DEFAULT_RETURN_PATH);
        assert_eq!(
            normalize_return_path(Some(" ")).unwrap(),
            DEFAULT_RETURN_PATH
        );
        assert_eq!(
            normalize_return_path(Some(" /admin/policies?google_drive=done ")).unwrap(),
            "/admin/policies?google_drive=done"
        );
    }

    #[test]
    fn normalize_return_path_rejects_external_or_ambiguous_paths() {
        assert!(normalize_return_path(Some("https://example.test/admin")).is_err());
        assert!(normalize_return_path(Some("//example.test/admin")).is_err());
        assert!(normalize_return_path(Some("/admin\\policies")).is_err());
        assert!(normalize_return_path(Some(&format!("/{}", "x".repeat(2049)))).is_err());
    }

    #[test]
    fn pkce_challenge_uses_sha256_urlsafe_base64() {
        assert_eq!(
            build_pkce_challenge("test-verifier"),
            "JBbiqONGWPaAmwXk_8bT6UnlPfrn65D32eZlJS-zGG0"
        );
    }

    #[test]
    fn authorization_url_contains_required_oauth_parameters() {
        let settings = GoogleDrivePolicyAuthSettings {
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
            scopes: "scope-a scope-b".to_string(),
        };
        let url = build_authorization_url(
            &settings,
            "https://app.example.test/api/v1/admin/policies/google-drive/oauth/callback",
            "state-token",
            "test-verifier",
        )
        .expect("authorization url should build");
        let parsed = url::Url::parse(&url).expect("authorization url should parse");
        let query = parsed
            .query_pairs()
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(
            parsed.as_str().split('?').next().unwrap(),
            "https://accounts.google.com/o/oauth2/v2/auth"
        );
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            query.get("client_id").map(String::as_str),
            Some("client-id")
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("scope-a scope-b")
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state-token"));
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("JBbiqONGWPaAmwXk_8bT6UnlPfrn65D32eZlJS-zGG0")
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(
            query.get("access_type").map(String::as_str),
            Some("offline")
        );
        assert_eq!(query.get("prompt").map(String::as_str), Some("consent"));
        assert_eq!(
            query.get("include_granted_scopes").map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn policy_auth_settings_trim_credentials_and_use_current_scope() {
        let policy = google_drive_policy(
            " client-id ",
            " client-secret ",
            StoragePolicyOptions {
                google_drive_use_app_data_folder: Some(true),
                ..Default::default()
            },
        );
        let options = StoragePolicyOptions {
            google_drive_use_app_data_folder: Some(true),
            ..Default::default()
        };
        let settings = policy_auth_settings(&policy, &options).unwrap();

        assert_eq!(settings.client_id, "client-id");
        assert_eq!(settings.client_secret, "client-secret");
        assert!(settings.scopes.contains("drive.appdata"));
        assert!(!settings.scopes.contains("/auth/drive "));
    }

    #[test]
    fn policy_auth_settings_reject_missing_credentials_with_google_subcode() {
        let missing_client = google_drive_policy(" ", "secret", StoragePolicyOptions::default());
        let error = match policy_auth_settings(&missing_client, &StoragePolicyOptions::default()) {
            Ok(_) => panic!("missing client id should fail"),
            Err(error) => error,
        };
        assert_eq!(
            error.api_error_subcode(),
            Some(ApiSubcode::GoogleDriveMisconfigured)
        );

        let missing_secret = google_drive_policy("client", " ", StoragePolicyOptions::default());
        let error = match policy_auth_settings(&missing_secret, &StoragePolicyOptions::default()) {
            Ok(_) => panic!("missing client secret should fail"),
            Err(error) => error,
        };
        assert_eq!(
            error.api_error_subcode(),
            Some(ApiSubcode::GoogleDriveMisconfigured)
        );
    }

    #[test]
    fn google_drive_error_reason_helpers_cover_token_and_quota_errors() {
        let auth_error = provider_error(json!({
            "error": "invalid_client",
            "error_description": "Bad client"
        }));
        assert!(google_drive_error_is_auth_failure(
            StatusCode::BAD_REQUEST,
            "Google Drive token exchange",
            Some(&auth_error),
        ));
        assert!(!google_drive_error_is_auth_failure(
            StatusCode::BAD_REQUEST,
            "Google Drive userinfo request",
            Some(&auth_error),
        ));

        let quota_error = provider_error(json!({
            "error": {
                "errors": [
                    {
                        "reason": "dailyLimitExceeded",
                        "message": "Daily limit exceeded"
                    }
                ]
            }
        }));
        assert!(google_drive_error_is_rate_limited(Some(&quota_error)));
    }

    #[test]
    fn sanitize_error_fragment_removes_control_chars_and_truncates() {
        let sanitized = sanitize_error_fragment(&format!(" ok\n{}\t", "x".repeat(300)));

        assert!(!sanitized.contains('\n'));
        assert!(!sanitized.contains('\t'));
        assert_eq!(sanitized.len(), 255);
        assert!(sanitized.starts_with("ok"));
    }

    #[test]
    fn normalize_optional_snapshot_trims_blank_values_and_caps_length() {
        assert_eq!(
            normalize_optional_snapshot(Some(" admin@example.com ")).as_deref(),
            Some("admin@example.com")
        );
        assert_eq!(normalize_optional_snapshot(Some(" ")), None);
        assert_eq!(
            normalize_optional_snapshot(Some(&"x".repeat(300)))
                .expect("snapshot should exist")
                .len(),
            255
        );
    }

    #[test]
    fn state_hash_is_deterministic_and_not_raw_state() {
        let hash = state_hash("state-token");

        assert_eq!(hash, state_hash("state-token"));
        assert_ne!(hash, "state-token");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn google_drive_policy_helper_serializes_options() {
        let policy = google_drive_policy(
            "client",
            "secret",
            StoragePolicyOptions {
                google_drive_root_folder_id: Some("root-folder".to_string()),
                ..Default::default()
            },
        );

        assert_ne!(policy.options, StoredStoragePolicyOptions::empty());
    }
}
