use super::common::api_not_found;
use super::routes;
use actix_web::web;

pub fn configure_primary(cfg: &mut web::ServiceConfig, db: &sea_orm::DatabaseConnection) {
    let route_config = crate::config::try_get_config()
        .map(|c| (*c).clone())
        .unwrap_or_default();
    let rl = &route_config.rate_limit;
    let network_trust = &route_config.network_trust;

    cfg.service(
        web::scope("/api/v1")
            .service(routes::auth::routes(rl, network_trust))
            .service(routes::files::routes(rl, network_trust))
            .service(routes::folders::routes(rl, network_trust))
            .service(routes::admin::routes(rl, network_trust))
            .service(routes::shares::routes(rl, network_trust))
            .service(routes::share_public::routes(rl, network_trust))
            .service(routes::webdav_accounts::routes(rl, network_trust))
            .service(routes::trash::routes(rl, network_trust))
            .service(routes::properties::routes(rl, network_trust))
            .service(routes::batch::routes(rl, network_trust))
            .service(routes::workspace_transfer::routes(rl, network_trust))
            .service(routes::search::routes(rl, network_trust))
            .service(routes::tags::routes(rl, network_trust))
            .service(routes::tasks::routes(rl, network_trust))
            .service(routes::teams::routes(rl, network_trust))
            .service(routes::public::routes())
            .service(routes::remote_tunnel::routes())
            .service(routes::wopi::routes())
            .default_service(web::to(api_not_found)),
    )
    .service(routes::health::primary_routes())
    .service(routes::share_public::direct_routes(rl, network_trust));

    #[cfg(all(debug_assertions, feature = "openapi"))]
    configure_openapi(cfg);

    if let Some(config) = crate::config::try_get_config() {
        crate::webdav::configure_with_rate_limit(
            cfg,
            &config.webdav,
            db,
            &config.rate_limit,
            &config.network_trust,
        );
    }

    cfg.service(routes::frontend::routes());
}

#[cfg(all(debug_assertions, feature = "openapi"))]
fn configure_openapi(cfg: &mut web::ServiceConfig) {
    use super::openapi;
    use actix_web::HttpResponse;
    use utoipa::OpenApi;
    use utoipa_swagger_ui::SwaggerUi;

    let spec = openapi::ApiDoc::openapi();
    let spec_clone = spec.clone();
    cfg.service(web::scope("/api-docs").route(
        "/openapi.json",
        web::get().to(move || {
            let s = spec_clone.clone();
            async move { HttpResponse::Ok().json(s) }
        }),
    ));
    cfg.service(SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", spec));
}
