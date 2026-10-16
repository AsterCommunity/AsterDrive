use super::common::api_not_found;
use super::routes;
use actix_web::web;

pub fn configure_follower(
    cfg: &mut web::ServiceConfig,
    observability: &crate::config::metrics::ObservabilityConfig,
) {
    cfg.service(
        web::scope("/api/v1")
            .service(routes::internal_storage::routes())
            .default_service(web::to(api_not_found)),
    )
    .service(routes::health::follower_routes(observability));
}
