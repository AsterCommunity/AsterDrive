//! Primary-side reverse tunnel endpoints for remote followers.

use crate::api::dto::validate_request;
use crate::api::extractors;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::storage::remote_protocol::tunnel::server::{
    self as tunnel, REMOTE_TUNNEL_JSON_LIMIT, REMOTE_TUNNEL_STREAM_FRAME_LIMIT,
    RemoteTunnelResponse,
};
use actix_web::{HttpRequest, HttpResponse, web};
use actix_ws::MessageStream;
use tokio_util::sync::CancellationToken;
use validator::Validate;

/// 读取有界请求 body：tunnel 控制面 JSON 整体入签前需要完整字节。
async fn read_limited_body(
    mut payload: web::Payload,
    limit: usize,
    subject: &str,
) -> Result<actix_web::web::Bytes> {
    use futures::StreamExt;

    let mut body = bytes::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let chunk = chunk
            .map_err(|error| AsterError::validation_error(format!("read {subject}: {error}")))?;
        if body.len() + chunk.len() > limit {
            return Err(AsterError::payload_too_large(format!(
                "{subject} body exceeds the {limit} byte limit"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

#[derive(Debug, serde::Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelPollReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub access_key: String,
}

pub fn routes() -> impl actix_web::dev::HttpServiceFactory + use<> {
    web::scope("/internal/remote-tunnel")
        .app_data(web::PayloadConfig::new(REMOTE_TUNNEL_JSON_LIMIT))
        .app_data(extractors::json_config(REMOTE_TUNNEL_JSON_LIMIT))
        .route("/poll", web::post().to(poll_remote_tunnel))
        .route("/complete", web::post().to(complete_remote_tunnel))
        .route("/connect", web::get().to(connect_remote_tunnel))
        .route(
            "/proxy/{remote_node_id}",
            web::post().to(proxy_remote_tunnel),
        )
}

pub async fn proxy_remote_tunnel(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    remote_node_id: web::Path<i64>,
    query: web::Query<tunnel::RemoteTunnelProxyQuery>,
    body: web::Payload,
) -> Result<HttpResponse> {
    tunnel::proxy_tunnel_request(
        state.get_ref(),
        &req,
        remote_node_id.into_inner(),
        query.into_inner(),
        body,
    )
    .await
}

pub async fn poll_remote_tunnel(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse> {
    let body = read_limited_body(payload, REMOTE_TUNNEL_JSON_LIMIT, "reverse tunnel poll").await?;
    let content_length = req
        .headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    // poll 的 body 是有界 JSON，整体入签后再反序列化。
    let remote_node = tunnel::authorize_tunnel_request(
        state.get_ref(),
        req.method(),
        tunnel::REMOTE_TUNNEL_POLL_PATH,
        req.headers(),
        content_length,
    )
    .await?;
    let body: RemoteTunnelPollReq = serde_json::from_slice(&body).map_err(|error| {
        AsterError::validation_error(format!("invalid reverse tunnel poll body: {error}"))
    })?;
    validate_request(&body)?;
    if body.access_key != remote_node.access_key {
        return Err(crate::errors::AsterError::auth_invalid_credentials(
            "reverse tunnel poll access_key does not match signed credentials",
        ));
    }
    let response = tunnel::poll(state.get_ref(), &remote_node).await?;
    Ok(tunnel::envelope_response(response))
}

pub async fn complete_remote_tunnel(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse> {
    let body = read_limited_body(
        payload,
        REMOTE_TUNNEL_JSON_LIMIT,
        "reverse tunnel completion",
    )
    .await?;
    let content_length = req
        .headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    // complete 的 body 是有界 JSON envelope，整体入签后再反序列化。
    let remote_node = tunnel::authorize_tunnel_request(
        state.get_ref(),
        req.method(),
        tunnel::REMOTE_TUNNEL_COMPLETE_PATH,
        req.headers(),
        content_length,
    )
    .await?;
    let body: RemoteTunnelResponse = serde_json::from_slice(&body).map_err(|error| {
        AsterError::validation_error(format!("invalid reverse tunnel completion body: {error}"))
    })?;
    tunnel::complete(state.get_ref(), &remote_node, body).await?;
    Ok(tunnel::empty_envelope_response())
}

pub async fn connect_remote_tunnel(
    state: web::Data<PrimaryAppState>,
    shutdown_token: web::Data<CancellationToken>,
    req: HttpRequest,
    body: web::Payload,
) -> Result<HttpResponse> {
    let remote_node = tunnel::authorize_tunnel_request(
        state.get_ref(),
        req.method(),
        tunnel::REMOTE_TUNNEL_CONNECT_PATH,
        req.headers(),
        None,
    )
    .await?;
    let (response, session, stream): (HttpResponse, actix_ws::Session, MessageStream) =
        actix_ws::handle(&req, body).map_err(|error| {
            AsterError::validation_error(format!("upgrade reverse tunnel websocket: {error}"))
        })?;
    let stream = stream.max_frame_size(REMOTE_TUNNEL_STREAM_FRAME_LIMIT);
    let shutdown_token = shutdown_token.get_ref().clone();
    actix_web::rt::spawn(async move {
        if let Err(error) = tunnel::connect_stream(
            state.get_ref(),
            remote_node,
            session,
            stream,
            shutdown_token,
        )
        .await
        {
            tracing::warn!("reverse tunnel streaming connection ended with error: {error}");
        }
    });
    Ok(response)
}
