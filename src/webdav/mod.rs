//! WebDAV 模块导出。

use std::sync::Arc;
use std::time::Instant;

pub mod auth;
pub mod backend;
mod capability;
mod deltav;
mod handlers;
mod observation;
mod responses;
pub mod system_file;

use actix_web::{HttpRequest, HttpResponse, web};

use crate::config::{NetworkTrustConfig, RateLimitConfig, WebDavConfig};
use crate::runtime::PrimaryAppState;
use crate::services::ops::audit;
use aster_forge_utils::numbers::u64_to_usize;
use aster_forge_webdav::{DavEvent, DavEventOutcome, DavEventSink, DavMethod, DavPath};

#[cfg(test)]
pub(crate) use aster_forge_webdav::encode_href;
pub(crate) use aster_forge_webdav::{
    child_relative_path, display_name, href_for_dav_path, href_for_relative,
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
    fn publish(&self, event: &DavEvent) -> Result<(), aster_forge_webdav::DavObservationError> {
        let destination = event
            .destination
            .as_ref()
            .map(DavPath::as_str)
            .unwrap_or("");
        let observations = event.observations;
        match event.outcome {
            DavEventOutcome::Succeeded { status } => tracing::debug!(
                operation = ?event.operation,
                source = %event.source.as_str(),
                destination,
                status,
                elapsed_ms = event.elapsed.as_millis(),
                bytes_received = ?observations.bytes_received,
                bytes_sent = ?observations.bytes_sent,
                requested_ranges = ?observations.requested_ranges,
                served_ranges = ?observations.served_ranges,
                resources = ?observations.resources,
                backend_open_count = ?observations.backend_open_count,
                backend_call_count = ?observations.backend_call_count,
                protocol_failure = ?observations.protocol_failure,
                stream = ?observations.stream,
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
                bytes_received = ?observations.bytes_received,
                bytes_sent = ?observations.bytes_sent,
                requested_ranges = ?observations.requested_ranges,
                served_ranges = ?observations.served_ranges,
                resources = ?observations.resources,
                backend_open_count = ?observations.backend_open_count,
                backend_call_count = ?observations.backend_call_count,
                protocol_failure = ?observations.protocol_failure,
                stream = ?observations.stream,
                "WebDAV operation failed"
            ),
        }
        Ok(())
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
    let request_target = match aster_forge_webdav::actix::request_target(&req, &webdav.prefix) {
        Ok(target) => target,
        Err(error) => return aster_forge_webdav::actix::protocol_error_response(error),
    };
    let capability_target = aster_forge_webdav::DavCapabilityTarget::new(
        request_target.target.clone(),
        request_target.target.as_str() == "/",
    );
    let capability_context = aster_forge_webdav::DavCapabilityContext {
        principal: Some(auth_result.scope.actor_user_id().to_string()),
    };
    let capability_provider = capability::DriveDavCapabilityProvider::new(&dav_fs);
    let capability_snapshot = match aster_forge_webdav::actix::capability_snapshot(
        &capability_provider,
        &capability_target,
        &capability_context,
    )
    .await
    {
        Ok(snapshot) => snapshot,
        Err(response) => return response,
    };
    if let Some(method) = DavMethod::from_name(req.method().as_str())
        && let Some(response) = deltav::immutable_method_rejection(method, &capability_snapshot)
    {
        return response;
    }
    let method = match aster_forge_webdav::actix::gate_request_method(&req, &capability_snapshot) {
        Ok(method) => method,
        Err(response) => return response,
    };
    let request_headers = match aster_forge_webdav::actix::converted_headers(req.headers()) {
        Ok(headers) => headers,
        Err(response) => return response,
    };
    let request_head = match aster_forge_webdav::DavRequestHead::parse_known_method(
        method,
        &request_target,
        &request_headers,
    ) {
        Ok(request_head) => request_head,
        Err(error) => return aster_forge_webdav::actix::protocol_error_response(error),
    };
    let body_policy = match capability_snapshot.body_policy(method, webdav.xml_payload_limit) {
        Ok(Some(policy)) => policy,
        Ok(None) | Err(_) => {
            return aster_forge_webdav::actix::into_response(
                aster_forge_webdav::method_not_allowed_response(&capability_snapshot),
            );
        }
    };

    let operation_started_at = Instant::now();
    let request_body =
        match aster_forge_webdav::actix::prepare_request_body(body_policy, &mut payload).await {
            Ok(body) => body,
            Err(error) => {
                let response = aster_forge_webdav::actix::into_response(
                    aster_forge_webdav::body_error_response(error),
                );
                let observation = observation::DavObservation::new(
                    request_head,
                    operation_started_at,
                    webdav.event_sink.clone(),
                );
                return observation::observe_response(response, observation);
            }
        };
    let observation = observation::DavObservation::new(
        request_head.clone(),
        operation_started_at,
        webdav.event_sink.clone(),
    );
    observation.add_bytes_received(
        u64::try_from(
            request_body
                .xml()
                .len()
                .saturating_add(request_body.bytes().len()),
        )
        .unwrap_or(u64::MAX),
    );
    let response = observation::scope(observation.clone(), async {
        match request_head.method {
            DavMethod::Options => aster_forge_webdav::actix::into_response(
                aster_forge_webdav::options_response(&capability_snapshot),
            ),
            DavMethod::Propfind => {
                handlers::properties::handle_propfind(
                    &request_head,
                    &dav_fs,
                    lock_system.as_ref(),
                    &webdav.prefix,
                    request_body.xml(),
                    &capability_snapshot,
                    handlers::properties::PROPFIND_MAXIMUM_DURATION,
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
                if capability_snapshot.declaration().versioning.state
                    == aster_forge_webdav::DavVersioningState::Version
                {
                    deltav::handle_version_get_head(
                        &req,
                        &request_head,
                        &dav_fs,
                        &webdav.prefix,
                        false,
                    )
                    .await
                } else {
                    handlers::transfer::handle_get_head(
                        &req,
                        &request_head,
                        &dav_fs,
                        lock_system.as_ref(),
                        &webdav.prefix,
                        false,
                        &capability_snapshot,
                    )
                    .await
                }
            }
            DavMethod::Head => {
                if capability_snapshot.declaration().versioning.state
                    == aster_forge_webdav::DavVersioningState::Version
                {
                    deltav::handle_version_get_head(
                        &req,
                        &request_head,
                        &dav_fs,
                        &webdav.prefix,
                        true,
                    )
                    .await
                } else {
                    handlers::transfer::handle_get_head(
                        &req,
                        &request_head,
                        &dav_fs,
                        lock_system.as_ref(),
                        &webdav.prefix,
                        true,
                        &capability_snapshot,
                    )
                    .await
                }
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
                    &capability_snapshot,
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
                    &request_headers,
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
                    &request_headers,
                    &dav_fs,
                    lock_system.as_ref(),
                    &webdav.prefix,
                    &system_file_policy,
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
            DavMethod::VersionControl => {
                deltav::handle_version_control(
                    &request_head,
                    &dav_fs,
                    lock_system.as_ref(),
                    &webdav.prefix,
                    request_body.xml(),
                    &capability_snapshot,
                )
                .await
            }
            DavMethod::Report => {
                deltav::handle_report(
                    &request_head,
                    &dav_fs,
                    lock_system.as_ref(),
                    &webdav.prefix,
                    request_body.xml(),
                    &capability_snapshot,
                )
                .await
            }
            DavMethod::Patch
            | DavMethod::Acl
            | DavMethod::Checkout
            | DavMethod::Checkin
            | DavMethod::Uncheckout
            | DavMethod::Mkworkspace
            | DavMethod::Update
            | DavMethod::Label
            | DavMethod::Merge
            | DavMethod::BaselineControl
            | DavMethod::Mkactivity
            | DavMethod::Search
            | DavMethod::Orderpatch
            | DavMethod::Mkredirectref
            | DavMethod::Updateredirectref
            | DavMethod::Bind
            | DavMethod::Unbind
            | DavMethod::Rebind
            | DavMethod::Post => aster_forge_webdav::actix::into_response(
                aster_forge_webdav::method_not_allowed_response(&capability_snapshot),
            ),
        }
    })
    .await;
    observation::observe_response(response, observation)
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
