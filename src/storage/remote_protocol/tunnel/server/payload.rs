use base64::{
    Engine as _, display::Base64Display, engine::general_purpose::STANDARD as BASE64_STANDARD,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::api::response::ApiResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelRequest {
    pub request_id: String,
    pub method: String,
    pub path_and_query: String,
    pub headers: Vec<(String, String)>,
    #[serde(with = "base64_bytes_body")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub body: Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelResponse {
    pub request_id: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    #[serde(with = "base64_body")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelPollRequest {
    pub access_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelPollResponse {
    pub request: Option<RemoteTunnelRequest>,
}

pub(crate) fn serialized_poll_response_len(
    request: &RemoteTunnelRequest,
) -> Result<usize, serde_json::Error> {
    let mut metadata_only = request.clone();
    metadata_only.body = Bytes::new();
    let metadata_len = serde_json::to_vec(&ApiResponse::ok(RemoteTunnelPollResponse {
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
