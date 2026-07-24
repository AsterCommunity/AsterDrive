//! AsterDrive WebDAV product adapters and route integration.

pub mod auth;
pub mod db_lock_system;
pub mod dir_entry;
mod download_audit;
pub mod file;
pub mod fs;
pub mod metadata;
pub mod path_resolver;
pub mod system_file;
mod version_provider;

use actix_web::{HttpRequest, HttpResponse, web};

use crate::config::{NetworkTrustConfig, RateLimitConfig, WebDavConfig};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit;
use aster_forge_utils::numbers::u64_to_usize;
use aster_forge_webdav::handler::WebDavHandler;
use aster_forge_webdav::responses;

const AUTH_REALM: &str = "AsterDrive WebDAV";

/// Route-level WebDAV configuration shared by authenticated requests.
pub struct WebDavState {
    pub prefix: String,
    pub xml_payload_limit: usize,
}

/// Authenticates the AsterDrive account, constructs product adapters, and then
/// delegates the complete protocol exchange to Forge.
pub async fn webdav_handler(
    req: HttpRequest,
    mut payload: web::Payload,
    state: web::Data<PrimaryAppState>,
    webdav: web::Data<WebDavState>,
) -> HttpResponse {
    if !state
        .get_ref()
        .runtime_config()
        .get_bool_or("webdav_enabled", true)
    {
        return responses::service_unavailable_text("WebDAV is disabled");
    }

    let auth_result = match auth::authenticate_webdav(&req, state.get_ref()).await {
        Ok(result) => result,
        Err(auth::WebdavAuthError::RateLimited { retry_after }) => {
            return responses::unauthorized_retry_after(AUTH_REALM, retry_after);
        }
        Err(auth::WebdavAuthError::Rejected) => return responses::unauthorized(AUTH_REALM),
    };

    let audit_info = audit::AuditRequestInfo::from_request(&req);
    let audit_ctx = audit_info.to_context(auth_result.scope.actor_user_id());
    let dav_fs = fs::AsterDavFs::new_with_audit(
        state.get_ref().clone(),
        Some(auth_result.account_id),
        auth_result.scope,
        auth_result.root_folder_id,
        audit_ctx.clone(),
    );
    let lock_system = db_lock_system::DbLockSystem::new_with_audit(
        state.get_ref().clone(),
        auth_result.scope,
        auth_result.root_folder_id,
        audit_ctx,
    );
    let version_provider =
        version_provider::AsterDavVersionProvider::new(state.get_ref().writer_db(), &auth_result);
    let name_policy =
        system_file::SystemFileBlockPolicy::from_runtime_config(state.get_ref().runtime_config());

    WebDavHandler::new(
        &webdav.prefix,
        webdav.xml_payload_limit,
        &dav_fs,
        lock_system.as_ref(),
        &version_provider,
        &name_policy,
    )
    .handle(&req, &mut payload)
    .await
}

/// Registers the AsterDrive route, authentication protection, and payload cap.
pub fn configure(
    cfg: &mut web::ServiceConfig,
    webdav_config: &WebDavConfig,
    db: &sea_orm::DatabaseConnection,
) {
    let config = crate::config::try_get_config()
        .map(|config| (*config).clone())
        .unwrap_or_default();
    configure_with_rate_limit(
        cfg,
        webdav_config,
        db,
        &config.rate_limit,
        &config.network_trust,
    );
}

pub fn configure_with_rate_limit(
    cfg: &mut web::ServiceConfig,
    webdav_config: &WebDavConfig,
    _db: &sea_orm::DatabaseConnection,
    rate_limit: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) {
    let payload_limit = u64_to_usize(webdav_config.payload_limit, "webdav.payload_limit")
        .unwrap_or_else(|_| {
            tracing::warn!(
                configured = webdav_config.payload_limit,
                platform_limit = usize::MAX,
                "webdav.payload_limit exceeds platform usize range; using platform limit"
            );
            usize::MAX
        });
    let webdav_state = web::Data::new(WebDavState {
        prefix: webdav_config.prefix.clone(),
        xml_payload_limit: u64_to_usize(
            webdav_config.xml_payload_limit,
            "webdav.xml_payload_limit",
        )
        .unwrap_or_else(|_| {
            tracing::warn!(
                configured = webdav_config.xml_payload_limit,
                platform_limit = usize::MAX,
                "webdav.xml_payload_limit exceeds platform usize range; using platform limit"
            );
            usize::MAX
        }),
    });

    let auth_protection = web::Data::new(auth::WebdavAuthProtection::new(
        rate_limit.enabled,
        &rate_limit.auth,
        &network_trust.trusted_proxies,
    ));

    cfg.app_data(webdav_state)
        .app_data(auth_protection)
        .service(
            web::scope(&webdav_config.prefix)
                .app_data(web::PayloadConfig::new(payload_limit))
                .default_service(web::to(webdav_handler)),
        );
}

#[cfg(test)]
mod handler_tests;
