//! WebDAV 模块导出。

use std::sync::Arc;
use std::time::{Duration, Instant};

pub mod auth;
pub mod backend;
mod handlers;
mod protocol;
mod responses;
pub mod system_file;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, web};

use crate::config::{NetworkTrustConfig, RateLimitConfig, WebDavConfig};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit;
use aster_forge_utils::numbers::u64_to_usize;
use aster_forge_webdav::{
    DavEvent, DavEventOutcome, DavEventSink, DavMethod, IfHeader, lock_conflict_response,
};
use aster_forge_webdav::{DavLockSystem, DavPath};

#[cfg(test)]
pub(crate) use aster_forge_webdav::encode_href;
pub(crate) use aster_forge_webdav::{
    child_relative_path, display_name, href_for_dav_path, href_for_relative, parent_relative_path,
};
pub(crate) use responses::fs_error_response;

/// WebDAV 共享状态（单例）
pub struct WebDavState {
    pub prefix: String,
    pub xml_payload_limit: usize,
    event_sink: Arc<dyn DavEventSink>,
}

#[derive(Debug, Default)]
struct TracingDavEventSink;

impl DavEventSink for TracingDavEventSink {
    fn publish(&self, event: &DavEvent) {
        let destination = event
            .destination
            .as_ref()
            .map(DavPath::as_str)
            .unwrap_or("");
        match event.outcome {
            DavEventOutcome::Succeeded { status } => tracing::debug!(
                operation = ?event.operation,
                source = %event.source.as_str(),
                destination,
                status,
                elapsed_ms = event.elapsed.as_millis(),
                "WebDAV operation completed"
            ),
            DavEventOutcome::Failed {
                status,
                backend_error,
            } => tracing::debug!(
                operation = ?event.operation,
                source = %event.source.as_str(),
                destination,
                status,
                backend_error = ?backend_error,
                elapsed_ms = event.elapsed.as_millis(),
                "WebDAV operation failed"
            ),
        }
    }
}

/// WebDAV handler — 所有协议方法都由自研分发层处理
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
        return responses::webdav_disabled();
    }

    let auth_result = match auth::authenticate_webdav(&req, state.get_ref()).await {
        Ok(result) => result,
        Err(auth::WebdavAuthError::RateLimited { retry_after }) => {
            return responses::unauthorized_retry_after(retry_after);
        }
        Err(auth::WebdavAuthError::Rejected) => return responses::unauthorized(),
    };
    let request_head = match aster_forge_webdav::actix::request_head(&req, &webdav.prefix) {
        Ok(Some(request_head)) => request_head,
        Ok(None) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::method_not_allowed_response(),
            );
        }
        Err(error) => return protocol::protocol_error_response(error),
    };

    let audit_info = audit::AuditRequestInfo::from_request(&req);
    let audit_ctx = audit_info.to_context(auth_result.scope.actor_user_id());

    let dav_fs = backend::AsterDavFs::new_with_audit(
        state.get_ref().clone(),
        Some(auth_result.account_id),
        auth_result.scope,
        auth_result.root_folder_id,
        audit_ctx.clone(),
    );
    let lock_system = backend::lock::DbLockSystem::new_with_audit(
        state.get_ref().clone(),
        auth_result.scope,
        auth_result.root_folder_id,
        audit_ctx,
    );

    let operation_started_at = Instant::now();
    let request_body = match aster_forge_webdav::actix::prepare_request_body(
        request_head.method,
        &mut payload,
        webdav.xml_payload_limit,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => {
            let response = aster_forge_webdav::actix::into_response(
                aster_forge_webdav::body_error_response(error),
            );
            webdav.event_sink.publish(&completed_event(
                &request_head,
                response.status(),
                operation_started_at.elapsed(),
            ));
            return response;
        }
    };
    let response = match request_head.method {
        DavMethod::Options => {
            aster_forge_webdav::actix::into_response(aster_forge_webdav::options_response())
        }
        DavMethod::Report => {
            handlers::deltav::handle_report(
                &request_head,
                request_body.xml(),
                state.get_ref().writer_db(),
                &auth_result,
                &webdav.prefix,
            )
            .await
        }
        DavMethod::VersionControl => {
            handlers::deltav::handle_version_control(
                &request_head,
                state.get_ref().writer_db(),
                &auth_result,
            )
            .await
        }
        DavMethod::Propfind => {
            handlers::properties::handle_propfind(
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                request_body.xml(),
            )
            .await
        }
        DavMethod::Proppatch => {
            handlers::properties::handle_proppatch(
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                request_body.xml(),
            )
            .await
        }
        DavMethod::Get => {
            handlers::transfer::handle_get_head(
                &req,
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                false,
            )
            .await
        }
        DavMethod::Head => {
            handlers::transfer::handle_get_head(
                &req,
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                true,
            )
            .await
        }
        DavMethod::Put => {
            let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                state.get_ref().runtime_config(),
            );
            handlers::transfer::handle_put(
                &req,
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                &mut payload,
            )
            .await
        }
        DavMethod::Mkcol => {
            let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                state.get_ref().runtime_config(),
            );
            handlers::resources::handle_mkcol(
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
            )
            .await
        }
        DavMethod::Delete => {
            handlers::resources::handle_delete(
                &req,
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
            )
            .await
        }
        DavMethod::Copy | DavMethod::Move => {
            let system_file_policy = system_file::SystemFileBlockPolicy::from_runtime_config(
                state.get_ref().runtime_config(),
            );
            handlers::resources::handle_copy_move(
                &req,
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                &system_file_policy,
                request_head.method == DavMethod::Move,
            )
            .await
        }
        DavMethod::Lock => {
            handlers::locks::handle_lock(
                &req,
                &request_head,
                &dav_fs,
                lock_system.as_ref(),
                &webdav.prefix,
                request_body.xml(),
            )
            .await
        }
        DavMethod::Unlock => {
            handlers::locks::handle_unlock(&req, &request_head, lock_system.as_ref()).await
        }
    };
    webdav.event_sink.publish(&completed_event(
        &request_head,
        response.status(),
        operation_started_at.elapsed(),
    ));
    response
}

fn completed_event(
    request_head: &aster_forge_webdav::DavRequestHead,
    status: StatusCode,
    elapsed: Duration,
) -> DavEvent {
    DavEvent {
        request_id: None,
        operation: request_head.method.operation(),
        source: request_head.target.clone(),
        destination: request_head
            .destination
            .as_ref()
            .map(|destination| destination.path.clone()),
        outcome: DavEventOutcome::from_status(status.as_u16(), None),
        elapsed,
    }
}

pub(crate) fn ensure_system_file_name_allowed(
    system_file_policy: &system_file::SystemFileBlockPolicy,
    relative: &str,
) -> Result<(), HttpResponse> {
    let name = display_name(relative);
    if name.is_empty() || !system_file_policy.is_blocked_name(name) {
        return Ok(());
    }

    Err(responses::system_file_name_blocked())
}

pub(crate) async fn ensure_unlocked(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    if_header: Option<&IfHeader>,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), HttpResponse> {
    for lock in lock_system.conflicting_locks(path, deep).await {
        let lock_href = href_for_dav_path(prefix, &lock.path);
        let submitted_tokens = protocol::submitted_lock_tokens_for_path(
            if_header,
            &lock_href,
            request_scheme,
            request_host,
        );
        if !submitted_tokens.iter().any(|token| token == &lock.token) {
            return Err(match lock_conflict_response(prefix, &lock.path) {
                Ok(response) => aster_forge_webdav::actix::into_response(response),
                Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
            });
        }
    }

    Ok(())
}

pub(crate) async fn ensure_parent_unlocked(
    lock_system: &dyn DavLockSystem,
    relative: &str,
    prefix: &str,
    if_header: Option<&IfHeader>,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), HttpResponse> {
    let Some(parent) = parent_relative_path(relative) else {
        return Ok(());
    };
    let parent_path = DavPath::new(&parent).map_err(|_| responses::invalid_request_path())?;
    ensure_unlocked(
        lock_system,
        &parent_path,
        false,
        prefix,
        if_header,
        request_scheme,
        request_host,
    )
    .await
}

pub(crate) fn decoded_path_string(path: &DavPath) -> String {
    path.as_str().to_string()
}

/// 注册 WebDAV 路由
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
        event_sink: Arc::new(TracingDavEventSink),
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
