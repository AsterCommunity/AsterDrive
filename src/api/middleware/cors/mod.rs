//! AsterDrive runtime CORS policy adapter.

mod constants;

use actix_web::{Error, HttpResponse, ResponseError, dev::ServiceRequest, web};
use aster_forge_middleware::actix::cors::{
    CorsMiddlewareError, CorsMiddlewareErrorKind, CorsRejection, RuntimeCors as ForgeRuntimeCors,
    RuntimeCorsConfig, RuntimeCorsPolicy,
};

use self::constants::{ALLOWED_HEADERS, ALLOWED_METHODS, EXPOSE_HEADERS};
use crate::errors::AsterError;
use crate::runtime::PrimaryAppState;

pub fn runtime_cors() -> ForgeRuntimeCors {
    ForgeRuntimeCors::new(
        RuntimeCorsConfig::new(resolve_runtime_policy, is_cors_exempt_path, map_cors_error)
            .rejection_response(cors_rejection_response)
            .allowed_methods(ALLOWED_METHODS.iter().copied())
            .allowed_headers(ALLOWED_HEADERS.iter().copied())
            .exposed_headers(EXPOSE_HEADERS.iter().copied())
            .additional_origin_schemes(
                crate::config::cors::BROWSER_EXTENSION_ORIGIN_SCHEMES
                    .iter()
                    .copied(),
            ),
    )
}

fn cors_rejection_response(reason: CorsRejection) -> HttpResponse {
    let message = match reason {
        CorsRejection::OriginNotAllowed => "CORS origin is not allowed",
        CorsRejection::MethodNotAllowed => "CORS preflight method is not allowed",
        CorsRejection::HeadersNotAllowed => "CORS preflight headers are not allowed",
    };
    AsterError::auth_forbidden(message).error_response()
}

fn resolve_runtime_policy(req: &ServiceRequest) -> Result<RuntimeCorsPolicy, Error> {
    let state = req
        .app_data::<web::Data<PrimaryAppState>>()
        .ok_or_else(|| AsterError::internal_error("PrimaryAppState not found"))?;
    Ok(crate::config::cors::runtime_cors_policy(
        state.runtime_config(),
    ))
}

fn is_cors_exempt_path(path: &str) -> bool {
    matches!(
        path,
        "/" | "/index.html" | "/favicon.svg" | "/manifest.webmanifest" | "/sw.js"
    ) || path.starts_with("/workbox-")
        || path.starts_with("/assets/")
        || path.starts_with("/static/")
        || path.starts_with("/pdfjs/")
}

fn map_cors_error(error: CorsMiddlewareError) -> Error {
    match error.kind() {
        CorsMiddlewareErrorKind::InvalidRequest => AsterError::validation_error(format!(
            "invalid CORS origin or preflight request: {}",
            error.message()
        ))
        .into(),
        CorsMiddlewareErrorKind::InvalidResponse => AsterError::internal_error(format!(
            "invalid CORS response headers: {}",
            error.message()
        ))
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, body::to_bytes, http::header, test};
    use aster_forge_middleware::actix::cors::CorsAllowedOrigins;

    #[actix_web::test]
    async fn response_header_failures_remain_internal_errors_and_preserve_diagnostic_boundary() {
        for opaque in [false, true] {
            let app = test::init_service(
                App::new().wrap(ForgeRuntimeCors::new(
                    RuntimeCorsConfig::new(
                        |_| {
                            Ok(RuntimeCorsPolicy {
                                enabled: true,
                                allowed_origins: CorsAllowedOrigins::List(vec![]),
                                allow_credentials: false,
                                max_age_secs: 0,
                            })
                        },
                        |_| false,
                        map_cors_error,
                    )
                    .rejection_response(move |reason| {
                        let mut response = cors_rejection_response(reason);
                        response.headers_mut().append(
                            header::VARY,
                            header::HeaderValue::from_static("Accept-Encoding"),
                        );
                        response.headers_mut().append(
                            header::VARY,
                            if opaque {
                                header::HeaderValue::from_bytes(b"\xff").unwrap()
                            } else {
                                header::HeaderValue::from_static("*")
                            },
                        );
                        response.headers_mut().insert(
                            header::ACCESS_CONTROL_ALLOW_ORIGIN,
                            header::HeaderValue::from_static("*"),
                        );
                        response.headers_mut().insert(
                            header::HeaderName::from_static("x-product"),
                            header::HeaderValue::from_static("preserved"),
                        );
                        response
                    }),
                )),
            )
            .await;
            let result = test::try_call_service(
                &app,
                test::TestRequest::get()
                    .uri("/probe")
                    .insert_header((header::ORIGIN, "https://blocked.example.com"))
                    .to_request(),
            )
            .await;
            if opaque {
                let response = result.unwrap_err().error_response();
                assert_eq!(response.status(), 500);
                let body = to_bytes(response.into_body()).await.unwrap();
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["code"], "internal_server_error");
                assert!(!body["msg"].as_str().unwrap().contains("response headers"));
                assert!(!body.to_string().contains("Vary"));
            } else {
                let response = result.unwrap();
                assert_eq!(response.status(), 403);
                assert!(
                    !response
                        .headers()
                        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                );
                assert_eq!(response.headers().get("x-product").unwrap(), "preserved");
                assert!(
                    response
                        .headers()
                        .get_all(header::VARY)
                        .any(|value| value == "*")
                );
                let body: serde_json::Value = test::read_body_json(response).await;
                assert_eq!(body["code"], "forbidden");
            }
        }
    }
}
