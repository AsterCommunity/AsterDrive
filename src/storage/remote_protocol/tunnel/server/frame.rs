use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::errors::{AsterError, Result};
use aster_drive_storage::StorageErrorKind;

pub(super) const REMOTE_TUNNEL_STREAM_META_LIMIT: usize = 64 * 1024;
pub(super) const REMOTE_TUNNEL_STREAM_FRAME_VERSION: u8 = 1;

/// Frame types carried on a reverse tunnel streaming lane; a well-formed
/// exchange is `RequestStart`/`RequestBody`*/`RequestEnd` followed by
/// `ResponseStart`/`ResponseBody`*/`ResponseEnd`, or `Error` at any point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteTunnelStreamFrameKind {
    /// Opens a request; carries method, target, headers, and content length.
    RequestStart,
    /// One chunk of the request body, primary to follower.
    RequestBody,
    /// Closes the request body.
    RequestEnd,
    /// Opens the response; carries status and headers.
    ResponseStart,
    /// One chunk of the response body, follower to primary.
    ResponseBody,
    /// Closes the response body.
    ResponseEnd,
    /// Aborts the request; `message` explains the failure to the peer.
    Error,
}

/// One frame on a reverse tunnel streaming lane. Only the fields relevant to
/// `kind` are populated; `request_id` ties together all frames of one request.
#[derive(Debug, Clone)]
pub struct RemoteTunnelStreamFrame {
    /// Which stage of the exchange this frame belongs to.
    pub kind: RemoteTunnelStreamFrameKind,
    /// Identifier of the request this frame belongs to.
    pub request_id: String,
    /// HTTP method; set on `RequestStart` only.
    pub method: Option<String>,
    /// Request target; set on `RequestStart` only.
    pub path_and_query: Option<String>,
    /// Header pairs; set on `RequestStart` and `ResponseStart`.
    pub headers: Vec<(String, String)>,
    /// Declared body length in bytes; set on `RequestStart` when known.
    pub content_length: Option<u64>,
    /// HTTP status code; set on `ResponseStart` only.
    pub status: Option<u16>,
    /// Diagnostic message; set on `Error` frames.
    pub message: Option<String>,
    /// Body bytes carried by this frame; empty on control frames.
    pub body: Bytes,
}

impl RemoteTunnelStreamFrame {
    pub(crate) fn error(request_id: String, message: String) -> Self {
        Self {
            kind: RemoteTunnelStreamFrameKind::Error,
            request_id,
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: Some(message),
            body: Bytes::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RemoteTunnelStreamFrameMeta {
    kind: RemoteTunnelStreamFrameKind,
    request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_and_query: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Encodes one frame as the version byte, a big-endian 64-bit metadata
/// length, the JSON metadata, then the raw body bytes. Fails when the metadata
/// exceeds the 64 KiB wire budget.
pub fn encode_stream_frame(frame: &RemoteTunnelStreamFrame) -> Result<Bytes> {
    let meta = RemoteTunnelStreamFrameMeta {
        kind: frame.kind,
        request_id: frame.request_id.clone(),
        method: frame.method.clone(),
        path_and_query: frame.path_and_query.clone(),
        headers: frame.headers.clone(),
        content_length: frame.content_length,
        status: frame.status,
        message: frame.message.clone(),
    };
    let meta = serde_json::to_vec(&meta).map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("encode reverse tunnel streaming frame metadata: {error}"),
        )
    })?;
    if meta.len() > REMOTE_TUNNEL_STREAM_META_LIMIT {
        return Err(crate::errors::storage_driver_error(
            StorageErrorKind::Precondition,
            "reverse tunnel streaming frame metadata is too large",
        ));
    }
    let meta_len = aster_forge_utils::numbers::usize_to_u64(
        meta.len(),
        "reverse tunnel streaming frame metadata length",
    )?;
    let mut bytes = Vec::with_capacity(1 + 8 + meta.len() + frame.body.len());
    bytes.push(REMOTE_TUNNEL_STREAM_FRAME_VERSION);
    bytes.extend_from_slice(&meta_len.to_be_bytes());
    bytes.extend_from_slice(&meta);
    bytes.extend_from_slice(&frame.body);
    Ok(Bytes::from(bytes))
}

/// Decodes a frame produced by [`encode_stream_frame`], validating the wire
/// version and metadata bounds before parsing the JSON header.
pub fn decode_stream_frame(bytes: Bytes) -> Result<RemoteTunnelStreamFrame> {
    if bytes.len() < 9 {
        return Err(AsterError::validation_error(
            "reverse tunnel streaming frame is too short",
        ));
    }
    let version = bytes[0];
    if version != REMOTE_TUNNEL_STREAM_FRAME_VERSION {
        return Err(AsterError::validation_error(format!(
            "unsupported reverse tunnel streaming frame version {version}"
        )));
    }
    let mut meta_len_bytes = [0u8; 8];
    meta_len_bytes.copy_from_slice(&bytes[1..9]);
    let meta_len_u64 = u64::from_be_bytes(meta_len_bytes);
    let meta_len = aster_forge_utils::numbers::u64_to_usize(
        meta_len_u64,
        "reverse tunnel streaming frame metadata length",
    )?;
    if meta_len > REMOTE_TUNNEL_STREAM_META_LIMIT {
        return Err(AsterError::validation_error(
            "reverse tunnel streaming frame metadata is too large",
        ));
    }
    let meta_end = 9usize.checked_add(meta_len).ok_or_else(|| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Precondition,
            "reverse tunnel streaming frame metadata length overflow",
        )
    })?;
    if bytes.len() < meta_end {
        return Err(AsterError::validation_error(
            "reverse tunnel streaming frame metadata is truncated",
        ));
    }
    let meta: RemoteTunnelStreamFrameMeta =
        serde_json::from_slice(&bytes[9..meta_end]).map_err(|error| {
            AsterError::validation_error(format!(
                "decode reverse tunnel streaming frame metadata: {error}"
            ))
        })?;
    Ok(RemoteTunnelStreamFrame {
        kind: meta.kind,
        request_id: meta.request_id,
        method: meta.method,
        path_and_query: meta.path_and_query,
        headers: meta.headers,
        content_length: meta.content_length,
        status: meta.status,
        message: meta.message,
        body: bytes.slice(meta_end..),
    })
}
