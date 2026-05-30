//! 外部认证 URL 校验辅助。

use url::Url;

use crate::errors::{AsterError, MapAsterErr, Result};

pub(crate) fn parse_url(
    value: &str,
    context: &str,
    error_fn: fn(String) -> AsterError,
) -> Result<Url> {
    Url::parse(value).map_aster_err_ctx(context, error_fn)
}

pub(crate) fn has_http_scheme(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

pub(crate) fn is_https_or_loopback_http(url: &Url) -> bool {
    url.scheme() == "https"
        || (url.scheme() == "http" && url.host_str().is_some_and(is_loopback_host))
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1"
}
