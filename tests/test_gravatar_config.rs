//! 集成测试：`gravatar_config`。

#[macro_use]
mod common;

use aster_drive::db::repository::config_repo;
use aster_drive::db::repository::user_repo;
use aster_drive::entities::user;
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::services::user::profile;
use aster_drive::types::AvatarSource;

async fn load_user_model(state: &PrimaryAppState, user_id: i64) -> user::Model {
    user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap()
}

#[actix_web::test]
async fn test_gravatar_default_url() {
    let state = common::setup().await;
    let user = aster_drive::services::auth::local::register(
        &state,
        "gravatar_default",
        "default@example.com",
        "password123",
    )
    .await
    .unwrap();

    // 设置 avatar source 为 gravatar
    profile::set_avatar_source(&state, user.id, AvatarSource::Gravatar)
        .await
        .unwrap();

    let user_model = load_user_model(&state, user.id).await;
    let info = profile::get_profile_info(
        &state,
        &user_model,
        profile::AvatarAudience::SelfUser,
    )
    .await
    .unwrap();

    assert_eq!(info.avatar.source, AvatarSource::Gravatar);
    let url = info.avatar.url_512.unwrap();
    assert!(
        url.starts_with("https://www.gravatar.com/avatar/"),
        "expected default gravatar URL, got: {url}"
    );
    assert!(url.contains("d=identicon"));
    assert!(url.contains("s=512"));
    assert!(url.contains("r=g"));
}

#[actix_web::test]
async fn test_gravatar_custom_base_url() {
    let state = common::setup().await;
    let user = aster_drive::services::auth::local::register(
        &state,
        "gravatar_custom",
        "custom@example.com",
        "password123",
    )
    .await
    .unwrap();

    // 设置自定义 gravatar base url
    let config = config_repo::upsert(
        state.writer_db(),
        "gravatar_base_url",
        "https://cravatar.cn/avatar",
        user.id,
    )
    .await
    .unwrap();
    state.runtime_config.apply(config);

    profile::set_avatar_source(&state, user.id, AvatarSource::Gravatar)
        .await
        .unwrap();

    let user_model = load_user_model(&state, user.id).await;
    let info = profile::get_profile_info(
        &state,
        &user_model,
        profile::AvatarAudience::SelfUser,
    )
    .await
    .unwrap();

    let url = info.avatar.url_512.unwrap();
    assert!(
        url.starts_with("https://cravatar.cn/avatar/"),
        "expected custom gravatar URL, got: {url}"
    );
    assert!(url.contains("d=identicon"));
}

#[actix_web::test]
async fn test_gravatar_empty_config_fallback() {
    let state = common::setup().await;
    let user = aster_drive::services::auth::local::register(
        &state,
        "gravatar_empty",
        "empty@example.com",
        "password123",
    )
    .await
    .unwrap();

    // 设置空字符串配置
    let config = config_repo::upsert(state.writer_db(), "gravatar_base_url", "", user.id)
        .await
        .unwrap();
    state.runtime_config.apply(config);

    profile::set_avatar_source(&state, user.id, AvatarSource::Gravatar)
        .await
        .unwrap();

    let user_model = load_user_model(&state, user.id).await;
    let info = profile::get_profile_info(
        &state,
        &user_model,
        profile::AvatarAudience::SelfUser,
    )
    .await
    .unwrap();

    let url = info.avatar.url_512.unwrap();
    assert!(
        url.starts_with("https://www.gravatar.com/avatar/"),
        "expected fallback to default URL for empty config, got: {url}"
    );
}

#[actix_web::test]
async fn test_gravatar_trailing_slash_normalization() {
    let state = common::setup().await;
    let user = aster_drive::services::auth::local::register(
        &state,
        "gravatar_slash",
        "slash@example.com",
        "password123",
    )
    .await
    .unwrap();

    // 配置带末尾斜杠
    let config = config_repo::upsert(
        state.writer_db(),
        "gravatar_base_url",
        "https://mirror.example.com/avatar/",
        user.id,
    )
    .await
    .unwrap();
    state.runtime_config.apply(config);

    profile::set_avatar_source(&state, user.id, AvatarSource::Gravatar)
        .await
        .unwrap();

    let user_model = load_user_model(&state, user.id).await;
    let info = profile::get_profile_info(
        &state,
        &user_model,
        profile::AvatarAudience::SelfUser,
    )
    .await
    .unwrap();

    let url = info.avatar.url_512.unwrap();
    assert!(
        url.starts_with("https://mirror.example.com/avatar/"),
        "expected normalized URL without double slash, got: {url}"
    );
    // path 部分不应出现 // (去掉 scheme 后检查)
    let after_scheme = url.strip_prefix("https://").unwrap();
    assert!(
        !after_scheme.contains("//"),
        "URL path should not contain double slashes: {url}"
    );
}

#[actix_web::test]
async fn test_gravatar_whitespace_only_config_fallback() {
    let state = common::setup().await;
    let user = aster_drive::services::auth::local::register(
        &state,
        "gravatar_ws",
        "whitespace@example.com",
        "password123",
    )
    .await
    .unwrap();

    // 配置只有空白字符
    let config = config_repo::upsert(state.writer_db(), "gravatar_base_url", "   ", user.id)
        .await
        .unwrap();
    state.runtime_config.apply(config);

    profile::set_avatar_source(&state, user.id, AvatarSource::Gravatar)
        .await
        .unwrap();

    let user_model = load_user_model(&state, user.id).await;
    let info = profile::get_profile_info(
        &state,
        &user_model,
        profile::AvatarAudience::SelfUser,
    )
    .await
    .unwrap();

    let url = info.avatar.url_512.unwrap();
    assert!(
        url.starts_with("https://www.gravatar.com/avatar/"),
        "expected fallback for whitespace-only config, got: {url}"
    );
}
