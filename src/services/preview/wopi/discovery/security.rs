use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{
    AsterError, MapAsterErr, Result, auth_forbidden_with_code, validation_error_with_code,
};
use crate::runtime::SharedRuntimeState;
use crate::services::preview::apps;
use crate::services::preview::wopi::proof::validate_wopi_proof;
use crate::services::preview::wopi::types::WopiRequestSource;

use super::cache::load_discovery;
use super::types::WopiAppConfig;
use super::url::{origin_from_url, trusted_origins_for_app};

pub(crate) fn ensure_request_source_allowed(
    app: &apps::PublicPreviewAppDefinition,
    request_source: &WopiRequestSource<'_>,
) -> Result<()> {
    // 这里只做“配置级来源收敛”，用于挡掉明显不可信的 Origin / Referer。
    // 真正的 Microsoft 365 proof-key 验签在 `ensure_request_proof_valid()`，
    // 两层一起才构成完整的 WOPI 请求来源校验。
    let trusted_origins = trusted_origins_for_app(app);
    if trusted_origins.is_empty() {
        return Ok(());
    }

    if let Some(origin) = request_source
        .origin
        .filter(|value| !value.trim().is_empty())
        .map(|value| crate::config::cors::normalize_origin(value, false))
        .transpose()
        .map_aster_err_with(|| {
            validation_error_with_code(
                ApiErrorCode::ValidationRequestOriginInvalid,
                "invalid Origin header",
            )
        })?
    {
        if trusted_origins.iter().any(|allowed| allowed == &origin) {
            return Ok(());
        }
        return Err(auth_forbidden_with_code(
            ApiErrorCode::WopiRequestOriginUntrusted,
            "untrusted WOPI request origin",
        ));
    }

    if let Some(referer) = request_source
        .referer
        .filter(|value| !value.trim().is_empty())
    {
        let referer_origin = origin_from_url(referer).ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::ValidationRequestRefererInvalid,
                "invalid Referer header",
            )
        })?;
        if trusted_origins
            .iter()
            .any(|allowed| allowed == &referer_origin)
        {
            return Ok(());
        }
        return Err(auth_forbidden_with_code(
            ApiErrorCode::WopiRequestRefererUntrusted,
            "untrusted WOPI request referer",
        ));
    }

    Ok(())
}

pub(crate) async fn ensure_request_proof_valid(
    state: &impl SharedRuntimeState,
    app_config: &WopiAppConfig,
    access_token: &str,
    request_source: &WopiRequestSource<'_>,
) -> Result<()> {
    let Some(discovery_url) = app_config.discovery_url.as_deref() else {
        return Ok(());
    };
    let discovery = load_discovery(state, discovery_url).await?;
    let Some(proof_keys) = discovery.proof_keys() else {
        return Ok(());
    };
    let public_url = request_source.public_url.as_deref().ok_or_else(|| {
        AsterError::internal_error(
            "WOPI proof validation requires a configured public_site_url request URL",
        )
    })?;

    // discovery 提供了 proof-key 时，说明该 provider 期望按照微软定义的
    // proof 头部签名请求。这里必须在 access_token 解析成功后、业务落库前验签。
    validate_wopi_proof(
        proof_keys,
        access_token,
        public_url,
        request_source.proof,
        request_source.proof_old,
        request_source.timestamp,
        Utc::now(),
    )
}
