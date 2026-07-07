use crate::config::branding;
use crate::config::site_url;
use crate::config::{auth_runtime, media_processing, operations};
use crate::db::repository::config_repo;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::preview::apps;
use crate::types::parse_storage_policy_options;
use moka::future::Cache;
use serde::Serialize;
use std::collections::BTreeMap;
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
    pub passkey_login_enabled: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicFrontendMediaConfig {
    pub image_preview_preference: media_processing::PublicImagePreviewPreference,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicFrontendConfig {
    pub version: i32,
    pub branding: PublicBranding,
    pub media: PublicFrontendMediaConfig,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicCustomConfig {
    pub entries: BTreeMap<String, String>,
}

pub fn get_public_branding(state: &impl SharedRuntimeState) -> PublicBranding {
    let runtime_config = state.runtime_config();
    let auth_policy = auth_runtime::RuntimeAuthPolicy::from_runtime_config(runtime_config);
    PublicBranding {
        title: branding::title_or_default(runtime_config),
        description: branding::description_or_default(runtime_config),
        favicon_url: branding::favicon_url_or_default(runtime_config),
        wordmark_dark_url: branding::wordmark_dark_url_or_default(runtime_config),
        wordmark_light_url: branding::wordmark_light_url_or_default(runtime_config),
        site_urls: site_url::public_site_urls(runtime_config),
        allow_user_registration: auth_policy.allow_user_registration,
        passkey_login_enabled: auth_policy.passkey_login_enabled,
    }
}

pub fn get_public_frontend_config(state: &impl SharedRuntimeState) -> PublicFrontendConfig {
    PublicFrontendConfig {
        version: 1,
        branding: get_public_branding(state),
        media: PublicFrontendMediaConfig {
            image_preview_preference: operations::frontend_image_preview_preference(
                state.runtime_config(),
            ),
        },
    }
}

pub fn get_public_preview_apps(state: &impl SharedRuntimeState) -> apps::PublicPreviewAppsConfig {
    apps::get_public_preview_apps(state)
}

pub async fn get_public_custom_config(
    state: &impl SharedRuntimeState,
    include_authenticated: bool,
) -> Result<PublicCustomConfig> {
    let entries = config_repo::find_visible_custom(state.reader_db(), include_authenticated)
        .await?
        .into_iter()
        .map(|config| (config.key, config.value))
        .collect();
    Ok(PublicCustomConfig { entries })
}

pub async fn get_public_media_data_support(
    state: &impl SharedRuntimeState,
) -> Result<media_processing::PublicMediaDataSupport> {
    let cache_key = public_media_data_support_cache_key(state);
    if let Some(cached) = PUBLIC_MEDIA_DATA_SUPPORT_CACHE.get(&cache_key).await {
        return Ok(cached);
    }

    let support = build_public_media_data_support(state)?;
    PUBLIC_MEDIA_DATA_SUPPORT_CACHE
        .insert(cache_key, support.clone())
        .await;
    Ok(support)
}

pub(crate) fn invalidate_public_media_data_support_cache() {
    PUBLIC_MEDIA_DATA_SUPPORT_CACHE.invalidate_all();
}

pub async fn get_public_thumbnail_support(
    state: &impl SharedRuntimeState,
) -> Result<media_processing::PublicThumbnailSupport> {
    let cache_key = public_thumbnail_support_cache_key(state);
    if let Some(cached) = PUBLIC_THUMBNAIL_SUPPORT_CACHE.get(&cache_key).await {
        return Ok(cached);
    }

    let support = build_public_thumbnail_support(state)?;
    PUBLIC_THUMBNAIL_SUPPORT_CACHE
        .insert(cache_key, support.clone())
        .await;
    Ok(support)
}

pub(crate) fn invalidate_public_thumbnail_support_cache() {
    PUBLIC_THUMBNAIL_SUPPORT_CACHE.invalidate_all();
}

fn build_public_thumbnail_support(
    state: &impl SharedRuntimeState,
) -> Result<media_processing::PublicThumbnailSupport> {
    let mut support = media_processing::public_thumbnail_support(state.runtime_config());
    let mut image_thumbnail_extensions = support
        .image_thumbnail
        .extensions
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    for policy in state.policy_snapshot().all_policies() {
        let options = parse_storage_policy_options(policy.options.as_ref());
        if !options.uses_storage_native_thumbnail() || options.thumbnail_extensions.is_empty() {
            continue;
        }

        // Public capability aggregation must stay descriptor-based: this endpoint
        // may scan every policy, and constructing drivers would warm remote clients.
        if crate::storage::connectors::storage_connector_supports_native_thumbnail(
            policy.driver_type,
        )? {
            image_thumbnail_extensions.extend(options.thumbnail_extensions);
        }
    }

    support.image_thumbnail.enabled = !image_thumbnail_extensions.is_empty();
    support.image_thumbnail.extensions = image_thumbnail_extensions.into_iter().collect();
    support.image_preview = support.image_thumbnail.clone();
    Ok(support)
}

fn build_public_media_data_support(
    state: &impl SharedRuntimeState,
) -> Result<media_processing::PublicMediaDataSupport> {
    let mut support = media_processing::public_media_data_support(state.runtime_config());
    if !support.enabled {
        return Ok(support);
    }

    let mut storage_native_extensions = BTreeSet::new();
    for policy in state.policy_snapshot().all_policies() {
        let options = parse_storage_policy_options(policy.options.as_ref());
        if !options.uses_storage_native_media_metadata()
            || options.media_metadata_extensions.is_empty()
        {
            continue;
        }

        // Same boundary as thumbnails: descriptor capabilities are the source of
        // truth for static support without instantiating policy drivers.
        if crate::storage::connectors::storage_connector_supports_native_media_metadata(
            policy.driver_type,
        )? {
            storage_native_extensions.extend(options.media_metadata_extensions);
        }
    }

    if storage_native_extensions.is_empty() {
        return Ok(support);
    }

    // Public support is a capability union: an extension listed here means at
    // least one enabled policy can resolve it, not that every policy can.
    merge_storage_native_media_metadata_support(
        &mut support.kinds.audio,
        &storage_native_extensions,
    );
    merge_storage_native_media_metadata_support(
        &mut support.kinds.video,
        &storage_native_extensions,
    );
    Ok(support)
}

fn merge_storage_native_media_metadata_support(
    kind_support: &mut media_processing::PublicMediaDataKindSupport,
    storage_native_extensions: &BTreeSet<String>,
) {
    if kind_support.enabled
        && kind_support.match_kind == media_processing::PublicMediaDataSupportMatch::Any
    {
        return;
    }

    let mut extensions = kind_support
        .extensions
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    extensions.extend(storage_native_extensions.iter().cloned());
    kind_support.enabled = !extensions.is_empty();
    kind_support.match_kind = media_processing::PublicMediaDataSupportMatch::Extensions;
    kind_support.extensions = extensions.into_iter().collect();
}

fn public_media_data_support_cache_key(state: &impl SharedRuntimeState) -> String {
    let mut hasher = DefaultHasher::new();
    state
        .runtime_config()
        .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
        .hash(&mut hasher);
    state
        .runtime_config()
        .get(operations::MEDIA_METADATA_ENABLED_KEY)
        .hash(&mut hasher);
    state
        .runtime_config()
        .get(operations::MEDIA_METADATA_MAX_SOURCE_BYTES_KEY)
        .hash(&mut hasher);
    hash_policy_snapshot_for_public_support(state, &mut hasher);

    format!(
        "{PUBLIC_MEDIA_DATA_SUPPORT_CACHE_KEY}:{:x}",
        hasher.finish()
    )
}

fn public_thumbnail_support_cache_key(state: &impl SharedRuntimeState) -> String {
    let mut hasher = DefaultHasher::new();
    state
        .runtime_config()
        .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
        .hash(&mut hasher);
    hash_policy_snapshot_for_public_support(state, &mut hasher);

    format!("{PUBLIC_THUMBNAIL_SUPPORT_CACHE_KEY}:{:x}", hasher.finish())
}

fn hash_policy_snapshot_for_public_support(
    state: &impl SharedRuntimeState,
    hasher: &mut DefaultHasher,
) {
    let mut policies = state.policy_snapshot().all_policies();
    policies.sort_by_key(|policy| policy.id);
    for policy in policies {
        policy.id.hash(hasher);
        policy.driver_type.as_str().hash(hasher);
        policy.endpoint.hash(hasher);
        policy.bucket.hash(hasher);
        policy.base_path.hash(hasher);
        policy.remote_node_id.hash(hasher);
        policy.options.as_ref().hash(hasher);
    }
}
