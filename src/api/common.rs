use super::api_error_code::ApiErrorCode;
use actix_web::HttpResponse;

pub(super) async fn api_not_found() -> HttpResponse {
    HttpResponse::NotFound().json(crate::api::response::failure(
        ApiErrorCode::EndpointNotFound,
        "endpoint not found",
    ))
}
