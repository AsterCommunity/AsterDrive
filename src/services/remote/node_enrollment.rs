//! 服务模块：`remote::node_enrollment`。

use std::time::Duration;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::master_binding_repo;
use crate::entities::master_binding as master_binding_entity;
use crate::errors::{AsterError, Result};
use crate::services::{remote::enrollment, remote::master_binding};
use crate::storage::remote_protocol::normalize_remote_base_url;
use crate::utils::OUTBOUND_HTTP_USER_AGENT;
use sea_orm::DatabaseConnection;
use serde::Deserialize;

pub const BOOTSTRAP_REMOTE_MASTER_URL_ENV: &str = "ASTER_BOOTSTRAP_REMOTE_MASTER_URL";
pub const BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV: &str = "ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN";
pub const FOLLOWER_DEFAULT_SERVER_HOST: &str = "0.0.0.0";

#[derive(Debug, Clone)]
pub struct NodeEnrollmentInput {
    pub master_url: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct NodeEnrollmentResult {
    pub action: &'static str,
    pub binding: master_binding_entity::Model,
}

#[derive(Debug, Clone)]
pub enum NodeEnrollmentBootstrapOutcome {
    NotConfigured,
    Enrolled { action: &'static str },
    AlreadyConfigured,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    code: ApiErrorCode,
    msg: String,
    data: Option<T>,
}

pub fn follower_seed_config() -> crate::config::Config {
    let mut default = crate::config::Config::default();
    default.server.start_mode = crate::config::node_mode::NodeRuntimeMode::Follower;
    default.server.host = FOLLOWER_DEFAULT_SERVER_HOST.to_string();
    default
}

pub fn prepare_follower_bootstrap_config() -> Result<Option<std::path::PathBuf>> {
    prepare_follower_bootstrap_config_with(&|name| std::env::var(name).ok(), &|default| {
        crate::config::ensure_default_config_for_current_dir(default)
    })
}

pub async fn enroll(
    db: &DatabaseConnection,
    input: NodeEnrollmentInput,
) -> Result<NodeEnrollmentResult> {
    let master_url = normalize_remote_base_url(&input.master_url)?;
    let bootstrap = redeem_enrollment(&master_url, &input.token).await?;

    let (binding, action) = crate::db::transaction::with_transaction(db, async |txn| {
        master_binding::upsert_from_enrollment(
            txn,
            master_binding::UpsertMasterBindingInput {
                name: bootstrap.remote_node_name.clone(),
                master_url: bootstrap.master_url.clone(),
                access_key: bootstrap.access_key.clone(),
                secret_key: bootstrap.secret_key.clone(),
                is_enabled: bootstrap.is_enabled,
            },
        )
        .await
    })
    .await?;

    ack_enrollment(&master_url, &bootstrap.ack_token).await?;

    Ok(NodeEnrollmentResult { action, binding })
}

pub async fn bootstrap_from_env_if_configured(
    db: &DatabaseConnection,
) -> Result<NodeEnrollmentBootstrapOutcome> {
    bootstrap_from_env_with(db, &|name| std::env::var(name).ok()).await
}

async fn bootstrap_from_env_with<F>(
    db: &DatabaseConnection,
    get_env: &F,
) -> Result<NodeEnrollmentBootstrapOutcome>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(input) = read_bootstrap_input(get_env)? else {
        return Ok(NodeEnrollmentBootstrapOutcome::NotConfigured);
    };
    let normalized_master_url = normalize_remote_base_url(&input.master_url)?;

    match enroll(db, input).await {
        Ok(result) => {
            tracing::info!(
                master_url = result.binding.master_url,
                binding_id = result.binding.id,
                "bootstrapped follower enrollment from environment"
            );
            Ok(NodeEnrollmentBootstrapOutcome::Enrolled {
                action: result.action,
            })
        }
        Err(error)
            if should_treat_bootstrap_error_as_already_configured(&error)
                && has_binding_for_master_url(db, &normalized_master_url).await? =>
        {
            tracing::info!(
                master_url = normalized_master_url,
                error = %error,
                "follower enrollment bootstrap env already matches an existing binding; skipping"
            );
            Ok(NodeEnrollmentBootstrapOutcome::AlreadyConfigured)
        }
        Err(error) => Err(error),
    }
}

fn prepare_follower_bootstrap_config_with<F, E>(
    get_env: &F,
    ensure_default_config: &E,
) -> Result<Option<std::path::PathBuf>>
where
    F: Fn(&str) -> Option<String>,
    E: Fn(&crate::config::Config) -> Result<std::path::PathBuf>,
{
    let Some(_) = read_bootstrap_input(get_env)? else {
        return Ok(None);
    };

    let path = ensure_default_config(&follower_seed_config())?;
    Ok(Some(path))
}

fn read_bootstrap_input<F>(get_env: &F) -> Result<Option<NodeEnrollmentInput>>
where
    F: Fn(&str) -> Option<String>,
{
    let master_url = env_string(get_env, BOOTSTRAP_REMOTE_MASTER_URL_ENV);
    let token = env_string(get_env, BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV);

    if master_url.is_none() && token.is_none() {
        return Ok(None);
    }

    let (Some(master_url), Some(token)) = (master_url, token) else {
        return Err(AsterError::validation_error(format!(
            "{} and {} must both be set to bootstrap follower enrollment",
            BOOTSTRAP_REMOTE_MASTER_URL_ENV, BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV
        )));
    };

    Ok(Some(NodeEnrollmentInput { master_url, token }))
}

fn env_string<F>(get_env: &F, name: &str) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    get_env(name).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn should_treat_bootstrap_error_as_already_configured(error: &AsterError) -> bool {
    matches!(
        error.message(),
        enrollment::ENROLLMENT_TOKEN_COMPLETED_MESSAGE
            | enrollment::ENROLLMENT_TOKEN_EXPIRED_MESSAGE
            | enrollment::ENROLLMENT_TOKEN_REPLACED_MESSAGE
    )
}

async fn has_binding_for_master_url(db: &DatabaseConnection, master_url: &str) -> Result<bool> {
    Ok(master_binding_repo::find_all(db)
        .await?
        .into_iter()
        .any(|binding| binding.master_url == master_url))
}

async fn redeem_enrollment(
    master_url: &str,
    token: &str,
) -> Result<enrollment::RemoteEnrollmentBootstrap> {
    let url = format!("{master_url}/api/v1/public/remote-enrollment/redeem");
    let response = node_enrollment_http_client()?
        .post(url)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .map_err(|error| {
            AsterError::config_error(format!(
                "failed to reach master enrollment endpoint: {error}"
            ))
        })?;

    parse_api_response(response, "master enrollment request").await
}

async fn ack_enrollment(master_url: &str, ack_token: &str) -> Result<()> {
    let url = format!("{master_url}/api/v1/public/remote-enrollment/ack");
    let response = node_enrollment_http_client()?
        .post(url)
        .json(&serde_json::json!({ "ack_token": ack_token }))
        .send()
        .await
        .map_err(|error| {
            AsterError::config_error(format!(
                "failed to reach master enrollment ack endpoint: {error}"
            ))
        })?;

    parse_empty_api_response(response, "master enrollment ack request").await
}

fn node_enrollment_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| {
            AsterError::internal_error(format!(
                "failed to build node enrollment HTTP client: {error}"
            ))
        })
}

async fn parse_api_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    action: &str,
) -> Result<T> {
    let status = response.status();
    let body = response.bytes().await.map_err(|error| {
        AsterError::config_error(format!("failed to read {action} response body: {error}"))
    })?;
    let envelope: ApiEnvelope<T> = serde_json::from_slice(&body).map_err(|error| {
        AsterError::config_error(format!("failed to parse {action} response: {error}"))
    })?;

    if !status.is_success() || envelope.code != ApiErrorCode::Success {
        let message = if envelope.msg.trim().is_empty() {
            format!("{action} failed with HTTP {status}")
        } else {
            envelope.msg
        };
        return Err(AsterError::validation_error(message));
    }

    envelope
        .data
        .ok_or_else(|| AsterError::config_error(format!("{action} response is missing data")))
}

async fn parse_empty_api_response(response: reqwest::Response, action: &str) -> Result<()> {
    let status = response.status();
    let body = response.bytes().await.map_err(|error| {
        AsterError::config_error(format!("failed to read {action} response body: {error}"))
    })?;
    let envelope: ApiEnvelope<serde_json::Value> =
        serde_json::from_slice(&body).map_err(|error| {
            AsterError::config_error(format!("failed to parse {action} response: {error}"))
        })?;

    if !status.is_success() || envelope.code != ApiErrorCode::Success {
        let message = if envelope.msg.trim().is_empty() {
            format!("{action} failed with HTTP {status}")
        } else {
            envelope.msg
        };
        return Err(AsterError::validation_error(message));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::node_mode::NodeRuntimeMode;
    use actix_web::{App, HttpResponse, HttpServer, web};
    use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct TestHttpServer {
        base_url: String,
        handle: actix_web::dev::ServerHandle,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl TestHttpServer {
        async fn stop(self) {
            self.handle.stop(true).await;
            let _ = self.task.await;
        }
    }

    async fn build_follower_test_db() -> DatabaseConnection {
        let db_path = std::env::temp_dir().join(format!(
            "asterdrive-node-enrollment-service-{}.db",
            uuid::Uuid::new_v4()
        ));
        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: database_url,
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("node enrollment test db should connect");
        crate::runtime::startup::initialize_database_state(
            &db,
            &crate::config::Config {
                server: crate::config::ServerConfig {
                    start_mode: NodeRuntimeMode::Follower,
                    ..Default::default()
                },
                ..Default::default()
            },
            NodeRuntimeMode::Follower,
        )
        .await
        .expect("node enrollment test db should initialize");
        db
    }

    async fn spawn_enrollment_server(
        redeem_status: actix_web::http::StatusCode,
        redeem_body: serde_json::Value,
        ack_count: Arc<AtomicUsize>,
    ) -> TestHttpServer {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("node enrollment test server should bind");
        let addr = listener
            .local_addr()
            .expect("node enrollment test server should expose addr");
        let ack_count_for_server = ack_count.clone();
        let server = HttpServer::new(move || {
            let redeem_status = redeem_status;
            let redeem_body = redeem_body.clone();
            let ack_count = ack_count_for_server.clone();
            App::new()
                .route(
                    "/api/v1/public/remote-enrollment/redeem",
                    web::post().to(move || {
                        let redeem_body = redeem_body.clone();
                        async move { HttpResponse::build(redeem_status).json(redeem_body) }
                    }),
                )
                .route(
                    "/api/v1/public/remote-enrollment/ack",
                    web::post().to(move || {
                        let ack_count = ack_count.clone();
                        async move {
                            ack_count.fetch_add(1, Ordering::Relaxed);
                            HttpResponse::Ok().json(serde_json::json!({
                                "code": "success",
                                "msg": "",
                                "data": null
                            }))
                        }
                    }),
                )
        })
        .listen(listener)
        .expect("node enrollment test server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);

        TestHttpServer {
            base_url: format!("http://127.0.0.1:{}", addr.port()),
            handle,
            task,
        }
    }

    #[test]
    fn read_bootstrap_input_requires_master_url_and_token_together() {
        let error = read_bootstrap_input(&|name| match name {
            BOOTSTRAP_REMOTE_MASTER_URL_ENV => Some("http://localhost:3000".to_string()),
            _ => None,
        })
        .expect_err("partial bootstrap env should be rejected");

        assert!(error.message().contains(BOOTSTRAP_REMOTE_MASTER_URL_ENV));
        assert!(
            error
                .message()
                .contains(BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV)
        );
    }

    #[test]
    fn prepare_follower_bootstrap_config_uses_follower_seed_defaults() {
        let path = prepare_follower_bootstrap_config_with(
            &|name| match name {
                BOOTSTRAP_REMOTE_MASTER_URL_ENV => Some("http://localhost:3000".to_string()),
                BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV => Some("enr_test".to_string()),
                _ => None,
            },
            &|default| {
                assert_eq!(
                    default.server.start_mode,
                    crate::config::node_mode::NodeRuntimeMode::Follower
                );
                assert_eq!(default.server.host, FOLLOWER_DEFAULT_SERVER_HOST);
                Ok(std::path::PathBuf::from("data/config.toml"))
            },
        )
        .expect("bootstrap config preparation should succeed")
        .expect("bootstrap config should be prepared");

        assert_eq!(path, std::path::PathBuf::from("data/config.toml"));
    }

    #[test]
    fn prepare_follower_bootstrap_config_is_noop_without_env() {
        let called = std::cell::Cell::new(false);
        let path = prepare_follower_bootstrap_config_with(&|_| None, &|_| {
            called.set(true);
            Ok(std::path::PathBuf::from("data/config.toml"))
        })
        .expect("empty bootstrap env should be ignored");

        assert!(path.is_none());
        assert!(!called.get());
    }

    #[tokio::test]
    async fn bootstrap_from_env_enrolls_and_acks() {
        let db = build_follower_test_db().await;
        let ack_count = Arc::new(AtomicUsize::new(0));
        let server = spawn_enrollment_server(
            actix_web::http::StatusCode::OK,
            serde_json::json!({
                "code": "success",
                "msg": "",
                "data": {
                    "remote_node_id": 7,
                    "remote_node_name": "docker-follower",
                    "master_url": "http://127.0.0.1:3000",
                    "access_key": "ak_test",
                    "secret_key": "sk_test",
                    "is_enabled": true,
                    "ack_token": "enr_ack_mock"
                }
            }),
            ack_count.clone(),
        )
        .await;

        let outcome = bootstrap_from_env_with(&db, &|name| match name {
            BOOTSTRAP_REMOTE_MASTER_URL_ENV => Some(server.base_url.clone()),
            BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV => Some("enr_test".to_string()),
            _ => None,
        })
        .await
        .expect("bootstrap env enrollment should succeed");

        let NodeEnrollmentBootstrapOutcome::Enrolled { action } = outcome else {
            panic!("bootstrap should enroll a new binding");
        };
        assert_eq!(action, "created");
        assert_eq!(ack_count.load(Ordering::Relaxed), 1);

        let stored = master_binding_repo::find_all(&db)
            .await
            .expect("stored bindings should load");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "docker-follower");
        assert_eq!(stored[0].access_key, "ak_test");
        assert!(stored[0].storage_namespace.starts_with("mb_"));

        server.stop().await;
    }

    #[tokio::test]
    async fn bootstrap_from_env_skips_completed_token_when_binding_already_exists() {
        let db = build_follower_test_db().await;
        let ack_count = Arc::new(AtomicUsize::new(0));
        let server = spawn_enrollment_server(
            actix_web::http::StatusCode::BAD_REQUEST,
            serde_json::json!({
                "code": "auth.token_expired",
                "msg": enrollment::ENROLLMENT_TOKEN_COMPLETED_MESSAGE,
                "data": null
            }),
            ack_count.clone(),
        )
        .await;
        master_binding_entity::ActiveModel {
            name: Set("docker-follower".to_string()),
            master_url: Set(server.base_url.clone()),
            access_key: Set("ak_existing".to_string()),
            secret_key: Set("sk_existing".to_string()),
            storage_namespace: Set("mb_existing".to_string()),
            is_enabled: Set(true),
            created_at: Set(chrono::Utc::now()),
            updated_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("existing binding should insert");

        let outcome = bootstrap_from_env_with(&db, &|name| match name {
            BOOTSTRAP_REMOTE_MASTER_URL_ENV => Some(server.base_url.clone()),
            BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN_ENV => Some("enr_completed".to_string()),
            _ => None,
        })
        .await
        .expect("completed token with existing binding should be treated as already configured");

        assert!(matches!(
            outcome,
            NodeEnrollmentBootstrapOutcome::AlreadyConfigured
        ));
        assert_eq!(ack_count.load(Ordering::Relaxed), 0);

        server.stop().await;
    }
}
