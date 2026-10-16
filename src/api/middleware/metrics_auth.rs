//! Authentication policy for the Forge exposition handler only.

use actix_web::{HttpRequest, HttpResponse, http::header};
use base64::{Engine, prelude::BASE64_STANDARD};

use crate::config::metrics::MetricsBasicAuthConfig;
use aster_forge_auth::PasswordHashRuntime;

/// One limiter per process, shared by HTTP workers and independent of login.
/// A permit remains owned by the blocking job even if the HTTP future is dropped.
pub struct MetricsAuth {
    config: MetricsBasicAuthConfig,
    runtime: PasswordHashRuntime,
}

impl MetricsAuth {
    pub fn new(config: MetricsBasicAuthConfig) -> Self {
        Self {
            config,
            runtime: PasswordHashRuntime::default(),
        }
    }

    pub async fn authorize(&self, request: &HttpRequest) -> bool {
        if !self.config.enabled {
            return true;
        }
        let Some((username, password)) = credentials(request) else {
            return false;
        };
        let result = self
            .runtime
            .try_verify_password(&password, &self.config.password_hash)
            .await;
        result.is_ok_and(|result| result.is_valid && username == self.config.username)
    }
}

fn credentials(request: &HttpRequest) -> Option<(String, String)> {
    let mut values = request.headers().get_all(header::AUTHORIZATION);
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() || value.len() > 8192 {
        return None;
    }
    let (scheme, encoded) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Basic") || encoded.is_empty() {
        return None;
    }
    let decoded = String::from_utf8(BASE64_STANDARD.decode(encoded).ok()?).ok()?;
    let (username, password) = decoded.split_once(':')?;
    if username.len() > 256
        || password.len() > 4096
        || username.chars().any(char::is_control)
        || password.chars().any(char::is_control)
    {
        return None;
    }
    Some((username.to_owned(), password.to_owned()))
}

pub fn unauthorized() -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header((
            header::WWW_AUTHENTICATE,
            "Basic realm=\"AsterDrive metrics\"",
        ))
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .finish()
}

pub async fn middleware(
    request: actix_web::dev::ServiceRequest,
    next: actix_web::middleware::Next<impl actix_web::body::MessageBody>,
) -> Result<
    actix_web::dev::ServiceResponse<actix_web::body::EitherBody<impl actix_web::body::MessageBody>>,
    actix_web::Error,
> {
    let auth = request
        .app_data::<actix_web::web::Data<MetricsAuth>>()
        .cloned();
    let authorized = match auth {
        Some(auth) => auth.authorize(request.request()).await,
        None => false,
    };
    if !authorized {
        return Ok(request.into_response(unauthorized()).map_into_right_body());
    }
    let mut response = next.call(request).await?.map_into_left_body();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test, web};
    use aster_forge_crypto::{
        PasswordHashPolicy, PasswordHashVerificationLimits, PasswordHashWorkFactor,
        hash_password_with_policy,
    };

    fn enabled_config() -> crate::config::metrics::ObservabilityConfig {
        let mut config = crate::config::metrics::ObservabilityConfig::default();
        config.prometheus.enabled = true;
        config
    }

    fn config() -> MetricsBasicAuthConfig {
        let policy = PasswordHashPolicy::new(
            PasswordHashWorkFactor::new(8, 1, 1, 32).unwrap(),
            PasswordHashVerificationLimits::default(),
        )
        .unwrap();
        MetricsBasicAuthConfig {
            enabled: true,
            username: "prometheus".into(),
            password_hash: hash_password_with_policy("pass:with:colons", &policy).unwrap(),
        }
    }

    #[actix_web::test]
    async fn both_roles_reject_all_invalid_credentials_before_export() {
        for routes in [
            crate::api::routes::health::primary_routes,
            crate::api::routes::health::follower_routes,
        ] {
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(MetricsAuth::new(config())))
                    .service(routes(&enabled_config())),
            )
            .await;
            for header_value in [
                None,
                Some("Bearer jwt".to_owned()),
                Some("Basic !!".to_owned()),
                Some("Basic".to_owned()),
                Some("Basic ".to_owned()),
                Some("Basic Og==".to_owned()),
                Some(format!("Basic {}", BASE64_STANDARD.encode("no-colon"))),
                Some(format!(
                    "Basic {}",
                    BASE64_STANDARD.encode("wrong:pass:with:colons")
                )),
                Some(format!(
                    "Basic {}",
                    BASE64_STANDARD.encode("prometheus:wrong")
                )),
                Some(format!(
                    "Basic {}",
                    BASE64_STANDARD.encode([0xff, b':', b'x'])
                )),
            ] {
                let mut req = test::TestRequest::get().uri("/health/metrics");
                if let Some(value) = header_value {
                    req = req.insert_header((header::AUTHORIZATION, value));
                }
                let response = test::call_service(&app, req.to_request()).await;
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
                assert_eq!(
                    response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
                    "Basic realm=\"AsterDrive metrics\""
                );
                assert_eq!(
                    response.headers().get(header::CACHE_CONTROL).unwrap(),
                    "no-store"
                );
                assert!(test::read_body(response).await.is_empty());
            }
            let value = format!(
                "Basic {}",
                BASE64_STANDARD.encode("prometheus:pass:with:colons")
            );
            let response = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri("/health/metrics")
                    .append_header((header::AUTHORIZATION, value.clone()))
                    .append_header((header::AUTHORIZATION, value))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            let health =
                test::call_service(&app, test::TestRequest::get().uri("/health").to_request())
                    .await;
            assert_eq!(health.status(), StatusCode::OK);
        }
    }

    #[actix_web::test]
    async fn valid_credentials_disabled_auth_and_no_store_keep_forge_contract() {
        let provider = aster_forge_metrics::MetricsProvider::builder()
            .with_prometheus()
            .unwrap()
            .build()
            .unwrap();
        let recorder = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
        recorder.record_file_upload("stream", "success");
        for routes in [
            crate::api::routes::health::primary_routes,
            crate::api::routes::health::follower_routes,
        ] {
            for enabled in [true, false] {
                let mut cfg = config();
                cfg.enabled = enabled;
                let app = test::init_service(
                    App::new()
                        .app_data(web::Data::new(provider.clone()))
                        .app_data(web::Data::new(MetricsAuth::new(cfg)))
                        .service(routes(&enabled_config())),
                )
                .await;
                let mut req = test::TestRequest::get().uri("/health/metrics");
                if enabled {
                    req = req.insert_header((
                        header::AUTHORIZATION,
                        format!(
                            "bAsIc {}",
                            BASE64_STANDARD.encode("prometheus:pass:with:colons")
                        ),
                    ));
                }
                let response = test::call_service(&app, req.to_request()).await;
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(
                    response.headers().get(header::CACHE_CONTROL).unwrap(),
                    "no-store"
                );
                assert_eq!(
                    response.headers().get(header::CONTENT_TYPE).unwrap(),
                    "text/plain; version=0.0.4; charset=utf-8"
                );
                assert!(!test::read_body(response).await.is_empty());
            }
        }
    }

    #[actix_web::test]
    async fn configuration_debug_redacts_credentials() {
        let cfg = config();
        let debug = format!("{cfg:?}");
        assert!(!debug.contains(&cfg.password_hash));
        assert!(!debug.contains("prometheus"));
        let auth = MetricsAuth::new(cfg);

        let request = test::TestRequest::get()
            .insert_header((
                header::AUTHORIZATION,
                format!(
                    "Basic {}",
                    BASE64_STANDARD.encode("prometheus:pass:with:colons")
                ),
            ))
            .to_http_request();
        assert!(auth.authorize(&request).await);
    }

    #[actix_web::test]
    async fn authentication_hides_unavailable_provider_and_rotation_uses_a_new_snapshot() {
        let first = config();
        let auth = MetricsAuth::new(first.clone());
        let valid = test::TestRequest::get()
            .insert_header((
                header::AUTHORIZATION,
                format!(
                    "Basic {}",
                    BASE64_STANDARD.encode("prometheus:pass:with:colons")
                ),
            ))
            .to_http_request();
        assert!(auth.authorize(&valid).await);
        let mut rotated = first;
        let policy = PasswordHashPolicy::new(
            PasswordHashWorkFactor::new(8, 1, 1, 32).unwrap(),
            PasswordHashVerificationLimits::default(),
        )
        .unwrap();
        rotated.password_hash = hash_password_with_policy("new-password", &policy).unwrap();
        let rotated = MetricsAuth::new(rotated);
        assert!(!rotated.authorize(&valid).await);
        assert!(
            rotated
                .authorize(
                    &test::TestRequest::get()
                        .insert_header((
                            header::AUTHORIZATION,
                            format!(
                                "Basic {}",
                                BASE64_STANDARD.encode("prometheus:new-password")
                            )
                        ))
                        .to_http_request()
                )
                .await
        );

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(auth))
                .service(crate::api::routes::health::primary_routes(&enabled_config())),
        )
        .await;
        let denied = test::call_service(
            &app,
            test::TestRequest::get().uri("/health/metrics").to_request(),
        )
        .await;
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        assert!(test::read_body(denied).await.is_empty());
        let unavailable = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/health/metrics")
                .insert_header((
                    header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        BASE64_STANDARD.encode("prometheus:pass:with:colons")
                    ),
                ))
                .to_request(),
        )
        .await;
        assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            unavailable.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }
}
