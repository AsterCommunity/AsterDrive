//! API 中间件：`auth`。

use actix_web::{
    Error, HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    web,
};
use futures::future::{LocalBoxFuture, Ready, ok};
use std::rc::Rc;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::middleware::csrf::{self, RequestSourceMode};
use crate::api::request_auth::{access_cookie_token, bearer_token};
use crate::errors::{AsterError, auth_forbidden_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::auth_service;

/// JWT 认证中间件
/// 优先从 cookie 取 token，fallback 到 Authorization: Bearer header
pub struct JwtAuth;

impl<S, B> Transform<S, ServiceRequest> for JwtAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = JwtAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(JwtAuthMiddleware {
            service: Rc::new(service),
        })
    }
}

pub struct JwtAuthMiddleware<S> {
    service: Rc<S>,
}

fn password_change_session_allowed(req: &ServiceRequest) -> bool {
    let path = req.path();
    matches!(
        (req.method().as_str(), path),
        ("GET", "/api/v1/auth/me")
            | ("PUT", "/api/v1/auth/password")
            | ("POST", "/api/v1/auth/logout")
    )
}

impl<S, B> Service<ServiceRequest> for JwtAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let state = req
                .app_data::<web::Data<PrimaryAppState>>()
                .ok_or_else(|| AsterError::internal_error("PrimaryAppState not found"))?;

            // 1. Cookie 优先
            // 2. Authorization: Bearer fallback
            let cookie_token = access_cookie_token(req.request());
            if cookie_token.is_some() && csrf::is_unsafe_method(req.method()) {
                csrf::ensure_service_request_source_allowed(
                    &req,
                    state.get_ref().runtime_config(),
                    RequestSourceMode::OptionalWhenPresent,
                )?;
                csrf::ensure_service_double_submit_token(&req)?;
            }

            let token = cookie_token.or_else(|| bearer_token(req.request()));

            match token {
                None => Err(AsterError::auth_token_missing("missing token").into()),
                Some(t) => match auth_service::authenticate_access_token(state.get_ref(), &t).await
                {
                    Ok((claims, snapshot)) => {
                        if claims.password_change && !password_change_session_allowed(&req) {
                            return Err(auth_forbidden_with_code(
                                ApiErrorCode::AuthPasswordChangeRequired,
                                "password change required",
                            )
                            .into());
                        }
                        tracing::Span::current().record("user_id", claims.user_id);
                        req.extensions_mut().insert(claims);
                        req.extensions_mut().insert(snapshot);
                        svc.call(req).await
                    }
                    Err(err) => Err(err.into()),
                },
            }
        })
    }
}
