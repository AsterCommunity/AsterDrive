//! API 中间件：`metrics`。

use actix_web::{
    Error,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    web,
};
use futures::future::{LocalBoxFuture, Ready, ok};
use std::rc::Rc;
use std::time::Instant;

use crate::metrics_core::SharedMetricsRecorder;

pub struct MetricsMiddleware;

impl<S, B> Transform<S, ServiceRequest> for MetricsMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = MetricsService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(MetricsService {
            service: Rc::new(service),
        })
    }
}

pub struct MetricsService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for MetricsService<S>
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
        let metrics = req
            .app_data::<web::Data<SharedMetricsRecorder>>()
            .map(|data| data.get_ref().clone())
            .unwrap_or_else(crate::metrics_core::NoopMetrics::arc);

        if !metrics.enabled() {
            return Box::pin(async move { svc.call(req).await });
        }

        let started_at = Instant::now();
        let method = req.method().clone();
        let route = route_label(&req);

        Box::pin(async move {
            match svc.call(req).await {
                Ok(resp) => {
                    metrics.record_http_request(
                        method.as_str(),
                        &route,
                        resp.status().as_u16(),
                        started_at.elapsed().as_secs_f64(),
                    );
                    Ok(resp)
                }
                Err(error) => {
                    metrics.record_http_request(
                        method.as_str(),
                        &route,
                        error.as_response_error().status_code().as_u16(),
                        started_at.elapsed().as_secs_f64(),
                    );
                    Err(error)
                }
            }
        })
    }
}

fn route_label(req: &ServiceRequest) -> String {
    req.match_pattern().unwrap_or_else(|| unmatched_route(req))
}

fn unmatched_route(req: &ServiceRequest) -> String {
    let path = req.path();
    if path.starts_with("/api/") {
        "unmatched_api".to_string()
    } else if path.starts_with("/health") {
        "unmatched_health".to_string()
    } else {
        "unmatched".to_string()
    }
}
