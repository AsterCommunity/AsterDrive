use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use tokio::io::AsyncRead;

use crate::errors::Result;
use aster_drive_model::entities::managed_follower;

use super::RemoteTunnelRegistry;

#[derive(Debug)]
pub struct RemoteTunnelHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

pub struct RemoteTunnelStreamHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Box<dyn AsyncRead + Unpin + Send>,
}

#[async_trait]
pub trait RemoteTunnelBroker: Send + Sync {
    async fn send_tunnel_request(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Bytes,
    ) -> Result<RemoteTunnelHttpResponse>;

    async fn send_tunnel_stream(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<RemoteTunnelStreamHttpResponse>;

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
