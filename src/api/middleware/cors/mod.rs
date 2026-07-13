//! AsterDrive runtime CORS policy adapter.

mod constants;

use actix_web::{Error, dev::ServiceRequest, web};
use aster_forge_actix_middleware::cors::{
    CorsMiddlewareError, CorsMiddlewareErrorKind, RuntimeCors as ForgeRuntimeCors,
    RuntimeCorsConfig, RuntimeCorsPolicy,
};

use self::constants::{ALLOWED_HEADERS, ALLOWED_METHODS, EXPOSE_HEADERS};
use crate::errors::AsterError;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};

pub fn runtime_cors() -> ForgeRuntimeCors {
    ForgeRuntimeCors::new(
        RuntimeCorsConfig::new(resolve_runtime_policy, is_cors_exempt_path, map_cors_error)
            .allowed_methods(ALLOWED_METHODS.iter().copied())
            .allowed_headers(ALLOWED_HEADERS.iter().copied())
            .exposed_headers(EXPOSE_HEADERS.iter().copied()),
    )
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
