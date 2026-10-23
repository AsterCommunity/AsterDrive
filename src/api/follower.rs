use super::common::api_not_found;
use super::routes;
use actix_web::web;

pub fn configure_follower(
    cfg: &mut web::ServiceConfig,
    observability: &crate::config::metrics::ObservabilityConfig,
) {
    // follower 侧同样以 app data 提供可信网络，internal 端点的审计语义与 primary 一致。
    let network_trust = crate::config::try_get_config()
        .map(|config| config.network_trust.clone())
        .unwrap_or_default();
    cfg.app_data(web::Data::new(
        crate::config::TrustedClientNetworks::from_config(&network_trust),
    ));

    cfg.service(
        web::scope("/api/v1")
            .service(routes::internal_storage::routes())
            .default_service(web::to(api_not_found)),
    )
    .service(routes::health::follower_routes(observability));
}
