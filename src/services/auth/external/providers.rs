use chrono::Utc;
use sea_orm::{ActiveValue::Set, IntoActiveModel};

use crate::api::pagination::load_offset_page;
use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::db::repository::external_auth_provider_repo;
use crate::entities::external_auth_provider;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::types::external_auth_provider::StoredExternalAuthProviderOptions;
use aster_forge_api::NullablePatch;
use aster_forge_api::OffsetPage;
use aster_forge_external_auth::providers::microsoft::{
    normalize_microsoft_tenant_input, normalize_microsoft_tenant_or_issuer_url,
};
use aster_forge_external_auth::{
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderKind,
    ExternalAuthProviderOptions, ExternalAuthProviderTestResult,
    MicrosoftExternalAuthProviderOptions, default_registry, normalize as external_auth_normalize,
    parse_external_auth_provider_options, serialize_external_auth_provider_options,
};
use aster_forge_utils::id;

use super::REDACTED_SECRET;
use super::normalize::{normalize_secret_create, normalize_secret_update};
use super::{
    AdminExternalAuthProviderInfo, CreateExternalAuthProviderInput, ExternalAuthProviderKindInfo,
    ExternalAuthProviderTestParamsInput, ExternalAuthPublicProvider,
    UpdateExternalAuthProviderInput,
};

fn descriptor_to_info(descriptor: ExternalAuthProviderDescriptor) -> ExternalAuthProviderKindInfo {
    ExternalAuthProviderKindInfo {
        kind: descriptor.kind,
        protocol: descriptor.protocol,
        display_name: descriptor.display_name.to_string(),
        description: descriptor.description.to_string(),
        default_scopes: descriptor.default_scopes.to_string(),
        issuer_url_required: descriptor.issuer_url_required,
        manual_endpoint_configuration_supported: descriptor.manual_endpoint_configuration_supported,
        authorization_url_required: descriptor.authorization_url_required,
        token_url_required: descriptor.token_url_required,
        userinfo_url_required: descriptor.userinfo_url_required,
        supports_discovery: descriptor.supports_discovery,
        supports_pkce: descriptor.supports_pkce,
        supports_email_verified_claim: descriptor.supports_email_verified_claim,
    }
}

pub(super) fn provider_to_public(
    model: external_auth_provider::Model,
) -> ExternalAuthPublicProvider {
    ExternalAuthPublicProvider {
        key: model.key,
        kind: model.provider_kind,
        display_name: model.display_name,
        icon_url: model.icon_url,
    }
}

fn nullable_patch_to_update<T>(value: NullablePatch<T>) -> Option<Option<T>> {
    match value {
        NullablePatch::Absent => None,
        NullablePatch::Null => Some(None),
        NullablePatch::Value(value) => Some(Some(value)),
    }
}

fn provider_to_admin(
    model: external_auth_provider::Model,
) -> Result<AdminExternalAuthProviderInfo> {
    let allowed_domains =
        external_auth_normalize::parse_allowed_domains(model.allowed_domains.as_deref())?;
    let options = admin_provider_options(
        model.provider_kind,
        model.options.as_ref(),
        model.issuer_url.as_deref(),
    )?;
    let issuer_url = admin_issuer_url(model.provider_kind, model.issuer_url, &options);
    let authorization_url = admin_manual_endpoint(model.provider_kind, model.authorization_url);
    let token_url = admin_manual_endpoint(model.provider_kind, model.token_url);
    let userinfo_url = admin_manual_endpoint(model.provider_kind, model.userinfo_url);
    Ok(AdminExternalAuthProviderInfo {
        id: model.id,
        key: model.key,
        provider_kind: model.provider_kind,
        protocol: model.protocol,
        display_name: model.display_name,
        icon_url: model.icon_url,
        options,
        issuer_url,
        authorization_url,
        token_url,
        userinfo_url,
        client_id: model.client_id,
        client_secret: model
            .client_secret
            .as_ref()
            .filter(|secret| !secret.is_empty())
            .map(|_| REDACTED_SECRET.to_string()),
        client_secret_configured: model
            .client_secret
            .as_ref()
            .is_some_and(|secret| !secret.is_empty()),
        scopes: model.scopes,
        enabled: model.enabled,
        auto_provision_enabled: model.auto_provision_enabled,
        auto_link_verified_email_enabled: model.auto_link_verified_email_enabled,
        require_email_verified: model.require_email_verified,
        subject_claim: model.subject_claim,
        username_claim: model.username_claim,
        display_name_claim: model.display_name_claim,
        email_claim: model.email_claim,
        email_verified_claim: model.email_verified_claim,
        groups_claim: model.groups_claim,
        avatar_url_claim: model.avatar_url_claim,
        allowed_domains,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

fn admin_provider_options(
    provider_kind: ExternalAuthProviderKind,
    raw_options: &str,
    legacy_issuer_url: Option<&str>,
) -> Result<ExternalAuthProviderOptions> {
    let mut options = parse_external_auth_provider_options(raw_options);
    if provider_kind == ExternalAuthProviderKind::Microsoft && options.microsoft.is_none() {
        let Some(legacy_issuer_url) = legacy_issuer_url else {
            return normalize_provider_options(provider_kind, options);
        };
        let Some(tenant) = microsoft_tenant_from_legacy_issuer_url(legacy_issuer_url) else {
            return Ok(options);
        };
        options.microsoft = Some(MicrosoftExternalAuthProviderOptions::new(tenant));
    }
    normalize_provider_options(provider_kind, options)
}

fn admin_issuer_url(
    provider_kind: ExternalAuthProviderKind,
    issuer_url: Option<String>,
    options: &ExternalAuthProviderOptions,
) -> Option<String> {
    match provider_kind {
        ExternalAuthProviderKind::Oidc | ExternalAuthProviderKind::GenericOAuth2 => issuer_url,
        ExternalAuthProviderKind::Microsoft => {
            if options.microsoft.is_some() {
                None
            } else {
                issuer_url
            }
        }
        ExternalAuthProviderKind::GitHub
        | ExternalAuthProviderKind::Google
        | ExternalAuthProviderKind::Qq => None,
    }
}

fn admin_manual_endpoint(
    provider_kind: ExternalAuthProviderKind,
    value: Option<String>,
) -> Option<String> {
    match provider_kind {
        ExternalAuthProviderKind::Oidc | ExternalAuthProviderKind::GenericOAuth2 => value,
        ExternalAuthProviderKind::GitHub
        | ExternalAuthProviderKind::Google
        | ExternalAuthProviderKind::Microsoft
        | ExternalAuthProviderKind::Qq => None,
    }
}

pub(super) fn external_auth_provider_config(
    provider: &external_auth_provider::Model,
) -> ExternalAuthProviderConfig {
    ExternalAuthProviderConfig {
        id: provider.id,
        key: provider.key.clone(),
        provider_kind: provider.provider_kind,
        protocol: provider.protocol,
        options: parse_external_auth_provider_options(provider.options.as_ref()),
        issuer_url: provider.issuer_url.clone(),
        authorization_url: provider.authorization_url.clone(),
        token_url: provider.token_url.clone(),
        userinfo_url: provider.userinfo_url.clone(),
        client_id: provider.client_id.clone(),
        client_secret: provider.client_secret.clone(),
        scopes: provider.scopes.clone(),
        subject_claim: provider.subject_claim.clone(),
        username_claim: provider.username_claim.clone(),
        display_name_claim: provider.display_name_claim.clone(),
        email_claim: provider.email_claim.clone(),
        email_verified_claim: provider.email_verified_claim.clone(),
        groups_claim: provider.groups_claim.clone(),
        avatar_url_claim: provider.avatar_url_claim.clone(),
        outbound_http_user_agent: Some(OUTBOUND_HTTP_USER_AGENT.to_string()),
    }
}

fn external_auth_provider_config_from_test_params(
    input: ExternalAuthProviderTestParamsInput,
) -> Result<ExternalAuthProviderConfig> {
    let descriptor = default_registry().descriptor_for(input.provider_kind)?;
    let options = normalize_provider_options_from_test_params(
        input.provider_kind,
        input.options,
        input.issuer_url.as_deref(),
    )?;
    Ok(ExternalAuthProviderConfig {
        id: 0,
        key: "draft".to_string(),
        provider_kind: input.provider_kind,
        protocol: descriptor.protocol,
        options,
        issuer_url: normalize_provider_issuer_url_input(
            input.provider_kind,
            input.issuer_url,
            descriptor.issuer_url_required,
            true,
        )?,
        authorization_url: external_auth_normalize::normalize_manual_endpoint_input(
            input.authorization_url,
            "authorization_url",
            descriptor.authorization_url_required,
            descriptor.manual_endpoint_configuration_supported,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?,
        token_url: external_auth_normalize::normalize_manual_endpoint_input(
            input.token_url,
            "token_url",
            descriptor.token_url_required,
            descriptor.manual_endpoint_configuration_supported,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?,
        userinfo_url: external_auth_normalize::normalize_manual_endpoint_input(
            input.userinfo_url,
            "userinfo_url",
            descriptor.userinfo_url_required,
            descriptor.manual_endpoint_configuration_supported,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?,
        client_id: external_auth_normalize::normalize_required_field(
            &input.client_id,
            "client_id",
            512,
        )?,
        client_secret: normalize_secret_create(input.client_secret),
        scopes: external_auth_normalize::normalize_scopes_with_default(
            input.scopes.as_deref(),
            descriptor.default_scopes,
            descriptor.protocol,
        )?,
        subject_claim: None,
        username_claim: None,
        display_name_claim: None,
        email_claim: None,
        email_verified_claim: None,
        groups_claim: None,
        avatar_url_claim: None,
        outbound_http_user_agent: Some(OUTBOUND_HTTP_USER_AGENT.to_string()),
    })
}

fn normalize_provider_issuer_url_input(
    provider_kind: ExternalAuthProviderKind,
    value: Option<String>,
    required: bool,
    allow_test_override: bool,
) -> Result<Option<String>> {
    match provider_kind {
        ExternalAuthProviderKind::Oidc | ExternalAuthProviderKind::GenericOAuth2 => {
            Ok(external_auth_normalize::normalize_issuer_url_input(
                value,
                required,
                super::EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN,
            )?)
        }
        ExternalAuthProviderKind::Microsoft if allow_test_override => {
            Ok(normalize_microsoft_tenant_or_issuer_url(value)?)
        }
        ExternalAuthProviderKind::Microsoft => normalize_specialized_issuer_url_input(
            provider_kind,
            value,
            "Use options.microsoft.tenant for Microsoft providers",
        ),
        ExternalAuthProviderKind::Google if allow_test_override => {
            Ok(external_auth_normalize::normalize_issuer_url_input(
                value,
                false,
                super::EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN,
            )?)
        }
        ExternalAuthProviderKind::GitHub
        | ExternalAuthProviderKind::Google
        | ExternalAuthProviderKind::Qq => normalize_specialized_issuer_url_input(
            provider_kind,
            value,
            "Dedicated external auth providers use fixed endpoints",
        ),
    }
}

fn normalize_specialized_issuer_url_input(
    provider_kind: ExternalAuthProviderKind,
    value: Option<String>,
    message: &str,
) -> Result<Option<String>> {
    if value.as_deref().is_none_or(|value| value.trim().is_empty()) {
        return Ok(None);
    }
    Err(AsterError::validation_error(format!(
        "issuer_url is not configurable for {} providers. {message}.",
        provider_kind.as_str()
    )))
}

fn normalize_provider_options(
    provider_kind: ExternalAuthProviderKind,
    mut options: ExternalAuthProviderOptions,
) -> Result<ExternalAuthProviderOptions> {
    options = options.normalized();
    match provider_kind {
        ExternalAuthProviderKind::Microsoft => {
            let tenant = normalize_microsoft_tenant_input(
                options
                    .microsoft
                    .as_ref()
                    .map(|options| options.tenant.clone()),
            )?;
            options.microsoft = Some(MicrosoftExternalAuthProviderOptions::new(tenant));
        }
        _ if options.microsoft.is_some() => {
            return Err(AsterError::validation_error(format!(
                "microsoft provider options are not valid for {} providers",
                provider_kind.as_str()
            )));
        }
        _ => {}
    }
    Ok(options)
}

fn normalize_provider_options_from_create(
    provider_kind: ExternalAuthProviderKind,
    options: Option<ExternalAuthProviderOptions>,
) -> Result<ExternalAuthProviderOptions> {
    let mut options = options.unwrap_or_default().normalized();
    if provider_kind == ExternalAuthProviderKind::Microsoft && options.microsoft.is_none() {
        let tenant = normalize_microsoft_tenant_input(None)?;
        options.microsoft = Some(MicrosoftExternalAuthProviderOptions::new(tenant));
    }
    normalize_provider_options(provider_kind, options)
}

fn normalize_provider_options_from_test_params(
    provider_kind: ExternalAuthProviderKind,
    options: Option<ExternalAuthProviderOptions>,
    issuer_url: Option<&str>,
) -> Result<ExternalAuthProviderOptions> {
    if provider_kind == ExternalAuthProviderKind::Microsoft && issuer_url.is_some() {
        return Ok(options.unwrap_or_default().normalized());
    }
    normalize_provider_options(provider_kind, options.unwrap_or_default())
}

fn microsoft_tenant_from_legacy_issuer_url(value: &str) -> Option<String> {
    normalize_microsoft_tenant_input(Some(value.to_string())).ok()
}

fn serialize_options(
    options: &ExternalAuthProviderOptions,
) -> Result<StoredExternalAuthProviderOptions> {
    serialize_external_auth_provider_options(options)
        .map(StoredExternalAuthProviderOptions)
        .map_err(|error| {
            AsterError::internal_error(format!("serialize external auth provider options: {error}"))
        })
}

fn default_require_email_verified(provider_kind: ExternalAuthProviderKind) -> bool {
    !matches!(
        provider_kind,
        ExternalAuthProviderKind::Microsoft | ExternalAuthProviderKind::Qq
    )
}

pub async fn list_public_providers(
    state: &impl SharedRuntimeState,
) -> Result<Vec<ExternalAuthPublicProvider>> {
    Ok(external_auth_provider_repo::find_enabled(state.writer_db())
        .await?
        .into_iter()
        .filter(|provider| default_registry().contains(provider.provider_kind.into()))
        .map(provider_to_public)
        .collect())
}

pub async fn list_public_providers_by_kind(
    state: &impl SharedRuntimeState,
    provider_kind: ExternalAuthProviderKind,
) -> Result<Vec<ExternalAuthPublicProvider>> {
    Ok(
        external_auth_provider_repo::find_enabled_by_kind(state.writer_db(), provider_kind)
            .await?
            .into_iter()
            .filter(|provider| default_registry().contains(provider.provider_kind.into()))
            .map(provider_to_public)
            .collect(),
    )
}

pub async fn list_admin_providers(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<AdminExternalAuthProviderInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        external_auth_provider_repo::find_paginated(
            state.writer_db(),
            limit,
            offset,
            default_registry().supported_kinds().map(Into::into),
        )
        .await
    })
    .await?;
    let items = page
        .items
        .into_iter()
        .map(provider_to_admin)
        .collect::<Result<Vec<_>>>()?;
    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

pub fn list_provider_kinds() -> Vec<ExternalAuthProviderKindInfo> {
    default_registry()
        .descriptors()
        .into_iter()
        .map(descriptor_to_info)
        .collect()
}

pub async fn get_admin_provider(
    state: &impl SharedRuntimeState,
    id: i64,
) -> Result<AdminExternalAuthProviderInfo> {
    let provider = external_auth_provider_repo::find_by_id(state.writer_db(), id).await?;
    if !default_registry().contains(provider.provider_kind.into()) {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    provider_to_admin(provider)
}

pub async fn create_provider(
    state: &impl SharedRuntimeState,
    input: CreateExternalAuthProviderInput,
) -> Result<AdminExternalAuthProviderInfo> {
    let descriptor = default_registry().descriptor_for(input.provider_kind)?;
    let key = id::new_best_effort_uuid("external auth provider key", |candidate| {
        let db = state.writer_db();
        let provider_kind = input.provider_kind;
        async move {
            let candidate_key = candidate.to_string();
            external_auth_provider_repo::find_by_kind_key(db, provider_kind, &candidate_key)
                .await
                .map(|provider| provider.is_some())
        }
    })
    .await?
    .to_string();
    let provider_kind = input.provider_kind;
    let legacy_issuer_url = input.issuer_url.clone();
    let display_name = external_auth_normalize::normalize_required_field(
        &input.display_name,
        "display_name",
        128,
    )?;
    let icon_url = external_auth_normalize::normalize_icon_url_input(
        input.icon_url,
        super::EXTERNAL_AUTH_URL_MAX_LEN,
    )?;
    let options = normalize_provider_options_from_create(provider_kind, input.options)?;
    let issuer_url = normalize_provider_issuer_url_input(
        provider_kind,
        legacy_issuer_url,
        descriptor.issuer_url_required,
        false,
    )?;
    let authorization_url = external_auth_normalize::normalize_manual_endpoint_input(
        input.authorization_url,
        "authorization_url",
        descriptor.authorization_url_required,
        descriptor.manual_endpoint_configuration_supported,
        super::EXTERNAL_AUTH_URL_MAX_LEN,
    )?;
    let token_url = external_auth_normalize::normalize_manual_endpoint_input(
        input.token_url,
        "token_url",
        descriptor.token_url_required,
        descriptor.manual_endpoint_configuration_supported,
        super::EXTERNAL_AUTH_URL_MAX_LEN,
    )?;
    let userinfo_url = external_auth_normalize::normalize_manual_endpoint_input(
        input.userinfo_url,
        "userinfo_url",
        descriptor.userinfo_url_required,
        descriptor.manual_endpoint_configuration_supported,
        super::EXTERNAL_AUTH_URL_MAX_LEN,
    )?;
    let client_id =
        external_auth_normalize::normalize_required_field(&input.client_id, "client_id", 512)?;
    let scopes = external_auth_normalize::normalize_scopes_with_default(
        input.scopes.as_deref(),
        descriptor.default_scopes,
        descriptor.protocol,
    )?;
    let allowed_domains =
        external_auth_normalize::normalize_allowed_domains(input.allowed_domains)?;
    let now = Utc::now();
    let model = external_auth_provider::ActiveModel {
        key: Set(key),
        display_name: Set(display_name),
        icon_url: Set(icon_url),
        provider_kind: Set(provider_kind),
        protocol: Set(descriptor.protocol),
        options: Set(serialize_options(&options)?),
        issuer_url: Set(issuer_url),
        authorization_url: Set(authorization_url),
        token_url: Set(token_url),
        userinfo_url: Set(userinfo_url),
        client_id: Set(client_id),
        client_secret: Set(normalize_secret_create(input.client_secret)),
        scopes: Set(scopes),
        enabled: Set(input.enabled.unwrap_or(true)),
        auto_provision_enabled: Set(input.auto_provision_enabled.unwrap_or(false)),
        auto_link_verified_email_enabled: Set(input
            .auto_link_verified_email_enabled
            .unwrap_or(false)),
        require_email_verified: Set(input
            .require_email_verified
            .unwrap_or_else(|| default_require_email_verified(provider_kind))),
        subject_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.subject_claim,
            "subject_claim",
        )?),
        username_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.username_claim,
            "username_claim",
        )?),
        display_name_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.display_name_claim,
            "display_name_claim",
        )?),
        email_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.email_claim,
            "email_claim",
        )?),
        email_verified_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.email_verified_claim,
            "email_verified_claim",
        )?),
        groups_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.groups_claim,
            "groups_claim",
        )?),
        avatar_url_claim: Set(external_auth_normalize::normalize_optional_claim(
            input.avatar_url_claim,
            "avatar_url_claim",
        )?),
        allowed_domains: Set(allowed_domains),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let provider = external_auth_provider_repo::create(state.writer_db(), model).await?;
    provider_to_admin(provider)
}

pub async fn update_provider(
    state: &impl SharedRuntimeState,
    id: i64,
    input: UpdateExternalAuthProviderInput,
) -> Result<AdminExternalAuthProviderInfo> {
    let existing = external_auth_provider_repo::find_by_id(state.writer_db(), id).await?;
    if !default_registry().contains(existing.provider_kind.into()) {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    let descriptor = default_registry().descriptor_for(existing.provider_kind)?;
    let mut active = existing.clone().into_active_model();
    if let Some(display_name) = input.display_name {
        active.display_name = Set(external_auth_normalize::normalize_required_field(
            &display_name,
            "display_name",
            128,
        )?);
    }
    if let Some(icon_url) = input.icon_url.and_then(nullable_patch_to_update) {
        active.icon_url = Set(external_auth_normalize::normalize_icon_url_input(
            icon_url,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?);
    }
    if let Some(issuer_url) = input.issuer_url.and_then(nullable_patch_to_update) {
        active.issuer_url = Set(normalize_provider_issuer_url_input(
            existing.provider_kind,
            issuer_url,
            descriptor.issuer_url_required,
            false,
        )?);
    }
    if let Some(options) = input.options {
        let options = normalize_provider_options(existing.provider_kind, options)?;
        active.options = Set(serialize_options(&options)?);
        if existing.provider_kind == ExternalAuthProviderKind::Microsoft {
            active.issuer_url = Set(None);
        }
    }
    if let Some(authorization_url) = input.authorization_url.and_then(nullable_patch_to_update) {
        active.authorization_url = Set(external_auth_normalize::normalize_manual_endpoint_input(
            authorization_url,
            "authorization_url",
            descriptor.authorization_url_required,
            descriptor.manual_endpoint_configuration_supported,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?);
    }
    if let Some(token_url) = input.token_url.and_then(nullable_patch_to_update) {
        active.token_url = Set(external_auth_normalize::normalize_manual_endpoint_input(
            token_url,
            "token_url",
            descriptor.token_url_required,
            descriptor.manual_endpoint_configuration_supported,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?);
    }
    if let Some(userinfo_url) = input.userinfo_url.and_then(nullable_patch_to_update) {
        active.userinfo_url = Set(external_auth_normalize::normalize_manual_endpoint_input(
            userinfo_url,
            "userinfo_url",
            descriptor.userinfo_url_required,
            descriptor.manual_endpoint_configuration_supported,
            super::EXTERNAL_AUTH_URL_MAX_LEN,
        )?);
    }
    if let Some(client_id) = input.client_id {
        active.client_id = Set(external_auth_normalize::normalize_required_field(
            &client_id,
            "client_id",
            512,
        )?);
    }
    if let Some(client_secret) = input.client_secret {
        active.client_secret = Set(normalize_secret_update(
            client_secret,
            existing.client_secret.clone(),
        ));
    }
    if let Some(scopes) = input.scopes {
        active.scopes = Set(external_auth_normalize::normalize_scopes(
            Some(&scopes),
            existing.protocol.into(),
        )?);
    }
    if let Some(enabled) = input.enabled {
        active.enabled = Set(enabled);
    }
    if let Some(value) = input.auto_provision_enabled {
        active.auto_provision_enabled = Set(value);
    }
    if let Some(value) = input.auto_link_verified_email_enabled {
        active.auto_link_verified_email_enabled = Set(value);
    }
    if let Some(value) = input.require_email_verified {
        active.require_email_verified = Set(value);
    }
    if let Some(value) = input.subject_claim.and_then(nullable_patch_to_update) {
        active.subject_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "subject_claim",
        )?);
    }
    if let Some(value) = input.username_claim.and_then(nullable_patch_to_update) {
        active.username_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "username_claim",
        )?);
    }
    if let Some(value) = input.display_name_claim.and_then(nullable_patch_to_update) {
        active.display_name_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "display_name_claim",
        )?);
    }
    if let Some(value) = input.email_claim.and_then(nullable_patch_to_update) {
        active.email_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "email_claim",
        )?);
    }
    if let Some(value) = input
        .email_verified_claim
        .and_then(nullable_patch_to_update)
    {
        active.email_verified_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "email_verified_claim",
        )?);
    }
    if let Some(value) = input.groups_claim.and_then(nullable_patch_to_update) {
        active.groups_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "groups_claim",
        )?);
    }
    if let Some(value) = input.avatar_url_claim.and_then(nullable_patch_to_update) {
        active.avatar_url_claim = Set(external_auth_normalize::normalize_optional_claim(
            value,
            "avatar_url_claim",
        )?);
    }
    if let Some(value) = input.allowed_domains.and_then(nullable_patch_to_update) {
        active.allowed_domains = Set(external_auth_normalize::normalize_allowed_domains(value)?);
    }
    active.updated_at = Set(Utc::now());

    let provider = external_auth_provider_repo::update(state.writer_db(), active).await?;
    provider_to_admin(provider)
}

pub async fn delete_provider(state: &impl SharedRuntimeState, id: i64) -> Result<()> {
    let provider = external_auth_provider_repo::find_by_id(state.writer_db(), id).await?;
    if !default_registry().contains(provider.provider_kind.into()) {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    external_auth_provider_repo::delete(state.writer_db(), id).await
}

pub async fn test_provider(
    state: &impl SharedRuntimeState,
    id: i64,
) -> Result<ExternalAuthProviderTestResult> {
    let provider = external_auth_provider_repo::find_by_id(state.writer_db(), id).await?;
    let runtime_provider = external_auth_provider_config(&provider);
    let result = default_registry()
        .driver_for_provider(&runtime_provider)?
        .test_provider(&runtime_provider)
        .await?;
    external_auth_provider_repo::touch_updated_at(state.writer_db(), id, Utc::now()).await?;
    Ok(result)
}

pub async fn test_provider_params(
    _state: &impl SharedRuntimeState,
    input: ExternalAuthProviderTestParamsInput,
) -> Result<ExternalAuthProviderTestResult> {
    let provider = external_auth_provider_config_from_test_params(input)?;
    Ok(default_registry()
        .driver_for_provider(&provider)?
        .test_provider(&provider)
        .await?)
}
