//! API 中间件：`admin`。

use actix_web::{
    Error, HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
};
use futures::future::{LocalBoxFuture, Ready, ok};
use std::rc::Rc;

use crate::api::subcode::ApiSubcode;
use crate::errors::{AsterError, auth_forbidden_with_subcode};
use crate::services::auth_service::AuthSnapshot;

/// 要求请求已经通过 JwtAuth，并且当前用户是管理员。
pub struct RequireAdmin;

impl<S, B> Transform<S, ServiceRequest> for RequireAdmin
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RequireAdminMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RequireAdminMiddleware {
            service: Rc::new(service),
        })
    }
}

pub struct RequireAdminMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for RequireAdminMiddleware<S>
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
            let is_admin = {
                let extensions = req.extensions();
                let Some(snapshot) = extensions.get::<AuthSnapshot>() else {
                    return Err(AsterError::internal_error(
                        "missing auth snapshot in request context",
                    )
                    .into());
                };

                snapshot.role.is_admin()
            };

            if !is_admin {
                return Err(auth_forbidden_with_subcode(
                    ApiSubcode::AuthAdminRequired,
                    "admin role required",
                )
                .into());
            }

            svc.call(req).await
        })
    }
}
