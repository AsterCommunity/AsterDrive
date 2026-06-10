//! 用户服务聚合入口。

mod admin;
mod models;
mod preferences;
mod queries;

pub use admin::{
    CreateUserInput, CreateUserOutput, ForceDeleteSummary, UpdateUserInput, create,
    create_with_audit, force_delete, force_delete_with_audit, update, update_with_audit,
};
pub use models::{
    MePartialResponse, MeResponse, MeResponseField, MeResponseFields, UpdatePreferencesReq,
    UserCore, UserInfo, UserListFilters, UserPreferences, UserSummary,
};
pub use preferences::{get_preferences, parse_preferences, update_preferences};
pub use queries::{
    get, get_me, get_me_partial, get_self_info, list_paginated, to_user_info, to_user_infos,
    to_user_summary, to_user_summary_with_profile, user_summaries_by_ids, user_summary_by_id,
};
