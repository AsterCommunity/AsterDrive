use crate::config::branding;
use crate::config::site_url;
use crate::config::{auth_runtime, media_processing, operations};
use crate::runtime::PrimaryAppState;
use crate::services::preview_app_service;
use crate::types::parse_storage_policy_options;
use moka::future::Cache;
use serde::Serialize;
use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;
use std::time::Duration;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const PUBLIC_SUPPORT_CACHE_TTL_SECS: u64 = 60;
pub const PUBLIC_CONFIG_CACHE_CONTROL: &str = "public, max-age=60";
const PUBLIC_MEDIA_DATA_SUPPORT_CACHE_KEY: &str = "public_media_data_support";
const PUBLIC_THUMBNAIL_SUPPORT_CACHE_KEY: &str = "public_thumbnail_support";

static PUBLIC_THUMBNAIL_SUPPORT_CACHE: LazyLock<
    Cache<String, media_processing::PublicThumbnailSupport>,
> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(128)
        .time_to_live(Duration::from_secs(PUBLIC_SUPPORT_CACHE_TTL_SECS))
        .build()
});

static PUBLIC_MEDIA_DATA_SUPPORT_CACHE: LazyLock<
    Cache<String, media_processing::PublicMediaDataSupport>,
> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(128)
        .time_to_live(Duration::from_secs(PUBLIC_SUPPORT_CACHE_TTL_SECS))
        .build()
});

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicBranding {
    pub title: String,
    pub description: String,
    pub favicon_url: String,
    pub wordmark_dark_url: String,
    pub wordmark_light_url: String,
    pub site_urls: Vec<String>,
    pub allow_user_registration: bool,
}

pub fn get_public_branding(state: &PrimaryAppState) -> PublicBranding {
    let auth_policy = auth_runtime::RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    PublicBranding {
        title: branding::title_or_default(&state.runtime_config),
        description: branding::description_or_default(&state.runtime_config),
        favicon_url: branding::favicon_url_or_default(&state.runtime_config),
        wordmark_dark_url: branding::wordmark_dark_url_or_default(&state.runtime_config),
        wordmark_light_url: branding::wordmark_light_url_or_default(&state.runtime_config),
        site_urls: site_url::public_site_urls(&state.runtime_config),
        allow_user_registration: auth_policy.allow_user_registration,
    }
}

pub fn get_public_preview_apps(
    state: &PrimaryAppState,
) -> preview_app_service::PublicPreviewAppsConfig {
    preview_app_service::get_public_preview_apps(state)
}

pub async fn get_public_media_data_support(
    state: &PrimaryAppState,
) -> media_processing::PublicMediaDataSupport {
    let cache_key = public_media_data_support_cache_key(state);
    if let Some(cached) = PUBLIC_MEDIA_DATA_SUPPORT_CACHE.get(&cache_key).await {
        return cached;
    }

    let support = media_processing::public_media_data_support(&state.runtime_config);
    PUBLIC_MEDIA_DATA_SUPPORT_CACHE
        .insert(cache_key, support.clone())
        .await;
    support
}

pub(crate) fn invalidate_public_media_data_support_cache() {
    PUBLIC_MEDIA_DATA_SUPPORT_CACHE.invalidate_all();
}

pub async fn get_public_thumbnail_support(
    state: &PrimaryAppState,
) -> media_processing::PublicThumbnailSupport {
    let cache_key = public_thumbnail_support_cache_key(state);
    if let Some(cached) = PUBLIC_THUMBNAIL_SUPPORT_CACHE.get(&cache_key).await {
        return cached;
    }

    let support = build_public_thumbnail_support(state);
    PUBLIC_THUMBNAIL_SUPPORT_CACHE
        .insert(cache_key, support.clone())
        .await;
    support
}

pub(crate) fn invalidate_public_thumbnail_support_cache() {
    PUBLIC_THUMBNAIL_SUPPORT_CACHE.invalidate_all();
}

fn build_public_thumbnail_support(
    state: &PrimaryAppState,
) -> media_processing::PublicThumbnailSupport {
    let mut support = media_processing::public_thumbnail_support(&state.runtime_config);
    let mut extensions = support.extensions.iter().cloned().collect::<BTreeSet<_>>();

    for policy in state.policy_snapshot.all_policies() {
        let options = parse_storage_policy_options(policy.options.as_ref());
        if !options.uses_storage_native_thumbnail() || options.thumbnail_extensions.is_empty() {
            continue;
        }

        match state.driver_registry.get_driver(&policy) {
            Ok(driver) if driver.as_native_thumbnail().is_some() => {
                extensions.extend(options.thumbnail_extensions);
            }
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(
                    policy_id = policy.id,
                    "skip storage-native thumbnail public support for policy: {error}"
                );
            }
        }
    }

    support.extensions = extensions.into_iter().collect();
    support
}

fn public_media_data_support_cache_key(state: &PrimaryAppState) -> String {
    let mut hasher = DefaultHasher::new();
    state
        .runtime_config
        .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
        .hash(&mut hasher);
    state
        .runtime_config
        .get(operations::MEDIA_METADATA_ENABLED_KEY)
        .hash(&mut hasher);
    state
        .runtime_config
        .get(operations::MEDIA_METADATA_MAX_SOURCE_BYTES_KEY)
        .hash(&mut hasher);

    format!(
        "{PUBLIC_MEDIA_DATA_SUPPORT_CACHE_KEY}:{:x}",
        hasher.finish()
    )
}

fn public_thumbnail_support_cache_key(state: &PrimaryAppState) -> String {
    let mut hasher = DefaultHasher::new();
    state
        .runtime_config
        .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
        .hash(&mut hasher);

    let mut policies = state.policy_snapshot.all_policies();
    policies.sort_by_key(|policy| policy.id);
    for policy in policies {
        policy.id.hash(&mut hasher);
        format!("{:?}", policy.driver_type).hash(&mut hasher);
        policy.endpoint.hash(&mut hasher);
        policy.bucket.hash(&mut hasher);
        policy.base_path.hash(&mut hasher);
        policy.remote_node_id.hash(&mut hasher);
        policy.options.as_ref().hash(&mut hasher);
    }

    format!("{PUBLIC_THUMBNAIL_SUPPORT_CACHE_KEY}:{:x}", hasher.finish())
}
