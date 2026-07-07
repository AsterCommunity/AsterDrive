//! 用户资料服务聚合入口。

mod avatar;
mod avatar_image;
mod avatar_storage;
mod info;
mod profile;
pub(crate) mod shared;

pub use avatar::{
    avatar_image_response, cleanup_avatar_upload, get_avatar_bytes, set_avatar_source,
    upload_avatar,
};
pub use info::{
    AvatarAudience, AvatarInfo, UserProfileInfo, build_profile_info,
    build_share_public_avatar_info, get_profile_info_map, resolve_gravatar_base_url,
};
pub use profile::{get_profile_info, get_wopi_user_info, update_profile, update_wopi_user_info};
