use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use tokio::io::AsyncRead;

use crate::errors::Result;
use aster_drive_model::entities::managed_follower;

use super::RemoteTunnelRegistry;

/// A follower's fully buffered tunnel response.
#[derive(Debug)]
pub struct RemoteTunnelHttpResponse {
    /// HTTP status the follower returned.
    pub status: StatusCode,
    /// Response headers to forward to the original caller.
    pub headers: HeaderMap,
    /// Response body, bounded by the buffered tunnel limit.
    pub body: Bytes,
}

/// A follower's tunnel response with a streaming body.
pub struct RemoteTunnelStreamHttpResponse {
    /// HTTP status the follower returned.
    pub status: StatusCode,
    /// Response headers to forward to the original caller.
    pub headers: HeaderMap,
    /// Response body; read to completion to drive the stream to the end frame.
    pub body: Box<dyn AsyncRead + Unpin + Send>,
}

/// Dispatch point for tunnel requests to a follower, behind either the local
/// registry or a clustered proxy hop.
#[async_trait]
pub trait RemoteTunnelBroker: Send + Sync {
    /// Sends one buffered request and awaits the follower's buffered response.
    async fn send_tunnel_request(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Bytes,
    ) -> Result<RemoteTunnelHttpResponse>;

    /// Sends one streaming request, forwarding `body` as it is read and
    /// returning the follower's response headers with a streamable body.
    async fn send_tunnel_stream(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<RemoteTunnelStreamHttpResponse>;

    /// Whether a streaming lane to this follower is currently connected;
    /// callers use this to choose between streaming and buffered dispatch.
    fn has_tunnel_stream_lane(&self, remote_node: &managed_follower::Model) -> bool;
}

#[async_trait]
impl RemoteTunnelBroker for RemoteTunnelRegistry {
    async fn send_tunnel_request(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Bytes,
    ) -> Result<RemoteTunnelHttpResponse> {
        self.send(
            remote_node,
            method,
            path_and_query,
            content_length,
            extra_headers,
            body,
        )
        .await
    }

    async fn send_tunnel_stream(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<RemoteTunnelStreamHttpResponse> {
        self.send_stream(
            remote_node,
            method,
            path_and_query,
            content_length,
            extra_headers,
            body,
        )
        .await
    }

    fn has_tunnel_stream_lane(&self, remote_node: &managed_follower::Model) -> bool {
        self.has_stream_lane(remote_node)
    }
}
