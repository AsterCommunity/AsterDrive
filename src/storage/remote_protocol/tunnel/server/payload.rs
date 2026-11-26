use base64::{
    Engine as _, display::Base64Display, engine::general_purpose::STANDARD as BASE64_STANDARD,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// One buffered HTTP request the primary hands a follower for local replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelRequest {
    /// Identifier the follower echoes back when completing the request.
    pub request_id: String,
    /// HTTP method to replay locally.
    pub method: String,
    /// Request target the follower authenticates and dispatches against.
    pub path_and_query: String,
    /// Header pairs to replay, including the primary's authentication headers.
    pub headers: Vec<(String, String)>,
    /// Request body, base64 on the wire for JSON framing.
    #[serde(with = "base64_bytes_body")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub body: Bytes,
}

/// A follower's replayed response for one [`RemoteTunnelRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelResponse {
    /// Identifier of the request this response answers.
    pub request_id: String,
    /// HTTP status produced by the follower's local replay.
    pub status: u16,
    /// Response header pairs to forward to the original caller.
    pub headers: Vec<(String, String)>,
    /// Response body, base64 on the wire; legacy array encoding still decodes.
    #[serde(with = "base64_body")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub body: Vec<u8>,
}

/// Follower poll request body.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelPollRequest {
    /// Access key identifying the polling follower; must match the request's
    /// signed credentials.
    pub access_key: String,
}

/// Follower poll response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelPollResponse {
    /// The next queued tunnel request, or `None` when nothing is pending.
    pub request: Option<RemoteTunnelRequest>,
}

pub(crate) fn serialized_poll_response_len(
    request: &RemoteTunnelRequest,
) -> Result<usize, serde_json::Error> {
    let mut metadata_only = request.clone();
    metadata_only.body = Bytes::new();
    let metadata_len =
        serde_json::to_vec(&crate::api::response::success(RemoteTunnelPollResponse {
            request: Some(metadata_only),
        }))?
        .len();
    let encoded_body_len = request
        .body
        .len()
        .checked_add(2)
        .and_then(|len| len.checked_div(3))
        .and_then(|groups| groups.checked_mul(4))
        .unwrap_or(usize::MAX);
    Ok(metadata_len.saturating_add(encoded_body_len))
}

mod base64_body {
    use super::*;
    use serde::{Deserializer, Serializer, de::Error as _};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum EncodedBody {
        Base64(String),
        LegacyArray(Vec<u8>),
    }

    pub fn serialize<S>(body: &[u8], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(body))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match EncodedBody::deserialize(deserializer)? {
            EncodedBody::Base64(value) => BASE64_STANDARD
                .decode(value)
                .map_err(|error| D::Error::custom(format!("invalid base64 body: {error}"))),
            EncodedBody::LegacyArray(body) => Ok(body),
        }
    }
}

mod base64_bytes_body {
    use super::*;
    use serde::{Deserializer, Serializer, de::Error as _};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum EncodedBody {
        Base64(String),
        LegacyArray(Vec<u8>),
    }

    pub fn serialize<S>(body: &Bytes, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&Base64Display::new(body.as_ref(), &BASE64_STANDARD))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        match EncodedBody::deserialize(deserializer)? {
            EncodedBody::Base64(value) => BASE64_STANDARD
                .decode(value)
                .map(Bytes::from)
                .map_err(|error| D::Error::custom(format!("invalid base64 body: {error}"))),
            EncodedBody::LegacyArray(body) => Ok(Bytes::from(body)),
        }
    }
}
