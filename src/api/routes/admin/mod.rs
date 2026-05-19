//! 管理员 API 路由聚合入口。

use crate::api::middleware::{admin::RequireAdmin, auth::JwtAuth, rate_limit};
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::web;

// DTO re-exports from unified dto/ module
pub use crate::api::dto::admin::{
    AdminAuditLogSortQuery, AdminCreateTeamReq, AdminListQuery, AdminLockListQuery,
    AdminPatchTeamReq, AdminPolicyGroupListQuery, AdminPolicyListQuery, AdminRemoteNodeListQuery,
    AdminShareListQuery, AdminTaskCleanupReq, AdminTaskListQuery, AdminTeamListQuery,
    AdminUserListQuery, CreatePolicyGroupReq, CreatePolicyReq, CreateRemoteNodeReq, CreateUserReq,
    DeletePolicyQuery, ExecuteConfigActionReq, ExecuteConfigActionResp, MigratePolicyGroupUsersReq,
    PatchPolicyGroupReq, PatchPolicyReq, PatchRemoteNodeReq, PatchUserReq, PolicyGroupItemReq,
    ResetUserPasswordReq, SetConfigReq, TestPolicyParamsReq, TestRemoteNodeParamsReq,
};

pub(crate) mod audit_logs;
pub(crate) mod common;
pub(crate) mod config;
pub(crate) mod external_auth;
pub(crate) mod locks;
pub(crate) mod overview;
pub(crate) mod policies;
pub(crate) mod remote_nodes;
pub(crate) mod shares;
pub(crate) mod tasks;
pub(crate) mod teams;
pub(crate) mod users;

pub use audit_logs::list_audit_logs;
pub use config::{
    config_schema, config_template_variables, delete_config, execute_config_action, get_config,
    list_config, set_config,
};
pub use external_auth::{
    create_external_auth_provider, delete_external_auth_provider, get_external_auth_provider,
    list_external_auth_provider_kinds, list_external_auth_providers, test_external_auth_provider,
    test_external_auth_provider_params, update_external_auth_provider,
};
pub use locks::{cleanup_expired_locks, force_unlock, list_locks};
pub use overview::get_overview;
pub use policies::{
    create_policy, create_policy_group, delete_policy, delete_policy_group, get_policy,
    get_policy_group, list_policies, list_policy_groups, migrate_policy_group_users,
    test_policy_connection, test_policy_params, update_policy, update_policy_group,
};
pub use remote_nodes::{
    create_remote_node, create_remote_node_enrollment_token, create_remote_node_ingress_profile,
    delete_remote_node, delete_remote_node_ingress_profile, get_remote_node,
    list_remote_node_ingress_profiles, list_remote_nodes, test_remote_node,
    test_remote_node_params, update_remote_node, update_remote_node_ingress_profile,
};
pub use shares::{admin_delete_share, list_all_shares};
pub use tasks::{cleanup_tasks, list_tasks};
pub use teams::{
    add_team_member, create_team, delete_team, delete_team_member, get_team, list_team_audit_logs,
    list_team_members, list_teams, patch_team_member, restore_team, update_team,
};
pub use users::{
    create_user, force_delete_user, get_user, get_user_avatar, list_users, reset_user_password,
    revoke_user_sessions, update_user,
};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.write, &network_trust.trusted_proxies);

    web::scope("/admin")
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .service(
            web::scope("").wrap(JwtAuth).service(
                web::scope("")
                    .wrap(RequireAdmin)
                    .route("/overview", web::get().to(get_overview))
                    // policies
                    .route("/policies", web::get().to(list_policies))
                    .route("/policies", web::post().to(create_policy))
                    .route("/policies/{id}", web::get().to(get_policy))
                    .route("/policies/{id}", web::patch().to(update_policy))
                    .route("/policies/{id}", web::delete().to(delete_policy))
                    .route(
                        "/policies/{id}/test",
                        web::post().to(test_policy_connection),
                    )
                    .route("/policies/test", web::post().to(test_policy_params))
                    // remote nodes
                    .route("/remote-nodes", web::get().to(list_remote_nodes))
                    .route("/remote-nodes", web::post().to(create_remote_node))
                    .route("/remote-nodes/{id}", web::get().to(get_remote_node))
                    .route("/remote-nodes/{id}", web::patch().to(update_remote_node))
                    .route("/remote-nodes/{id}", web::delete().to(delete_remote_node))
                    .route("/remote-nodes/{id}/test", web::post().to(test_remote_node))
                    .route(
                        "/remote-nodes/{id}/enrollment-token",
                        web::post().to(create_remote_node_enrollment_token),
                    )
                    .route(
                        "/remote-nodes/{id}/ingress-profiles",
                        web::get().to(list_remote_node_ingress_profiles),
                    )
                    .route(
                        "/remote-nodes/{id}/ingress-profiles",
                        web::post().to(create_remote_node_ingress_profile),
                    )
                    .route(
                        "/remote-nodes/{id}/ingress-profiles/{profile_key}",
                        web::patch().to(update_remote_node_ingress_profile),
                    )
                    .route(
                        "/remote-nodes/{id}/ingress-profiles/{profile_key}",
                        web::delete().to(delete_remote_node_ingress_profile),
                    )
                    .route(
                        "/remote-nodes/test",
                        web::post().to(test_remote_node_params),
                    )
                    // external auth
                    .route(
                        "/external-auth/provider-kinds",
                        web::get().to(list_external_auth_provider_kinds),
                    )
                    .route(
                        "/external-auth/providers",
                        web::get().to(list_external_auth_providers),
                    )
                    .route(
                        "/external-auth/providers",
                        web::post().to(create_external_auth_provider),
                    )
                    .route(
                        "/external-auth/providers/test",
                        web::post().to(test_external_auth_provider_params),
                    )
                    .route(
                        "/external-auth/providers/{id}",
                        web::get().to(get_external_auth_provider),
                    )
                    .route(
                        "/external-auth/providers/{id}",
                        web::patch().to(update_external_auth_provider),
                    )
                    .route(
                        "/external-auth/providers/{id}",
                        web::delete().to(delete_external_auth_provider),
                    )
                    .route(
                        "/external-auth/providers/{id}/test",
                        web::post().to(test_external_auth_provider),
                    )
                    // policy groups
                    .route("/policy-groups", web::get().to(list_policy_groups))
                    .route("/policy-groups", web::post().to(create_policy_group))
                    .route("/policy-groups/{id}", web::get().to(get_policy_group))
                    .route("/policy-groups/{id}", web::patch().to(update_policy_group))
                    .route("/policy-groups/{id}", web::delete().to(delete_policy_group))
                    .route(
                        "/policy-groups/{id}/migrate-users",
                        web::post().to(migrate_policy_group_users),
                    )
                    // users
                    .route("/users", web::get().to(list_users))
                    .route("/users", web::post().to(create_user))
                    .route("/users/{id}", web::get().to(get_user))
                    .route("/users/{id}", web::patch().to(update_user))
                    .route("/users/{id}/password", web::put().to(reset_user_password))
                    .route(
                        "/users/{id}/sessions/revoke",
                        web::post().to(revoke_user_sessions),
                    )
                    .route("/users/{id}", web::delete().to(force_delete_user))
                    .route("/users/{id}/avatar/{size}", web::get().to(get_user_avatar))
                    // teams
                    .route("/teams", web::get().to(list_teams))
                    .route("/teams", web::post().to(create_team))
                    .route("/teams/{id}", web::get().to(get_team))
                    .route("/teams/{id}", web::patch().to(update_team))
                    .route("/teams/{id}", web::delete().to(delete_team))
                    .route("/teams/{id}/restore", web::post().to(restore_team))
                    .route(
                        "/teams/{id}/audit-logs",
                        web::get().to(list_team_audit_logs),
                    )
                    .route("/teams/{id}/members", web::get().to(list_team_members))
                    .route("/teams/{id}/members", web::post().to(add_team_member))
                    .route(
                        "/teams/{id}/members/{member_user_id}",
                        web::patch().to(patch_team_member),
                    )
                    .route(
                        "/teams/{id}/members/{member_user_id}",
                        web::delete().to(delete_team_member),
                    )
                    // shares
                    .route("/shares", web::get().to(list_all_shares))
                    .route("/shares/{id}", web::delete().to(admin_delete_share))
                    // tasks
                    .route("/tasks", web::get().to(list_tasks))
                    .route("/tasks/cleanup", web::post().to(cleanup_tasks))
                    // config
                    .route("/config", web::get().to(list_config))
                    .route("/config/schema", web::get().to(config_schema))
                    .route(
                        "/config/template-variables",
                        web::get().to(config_template_variables),
                    )
                    .route("/config/{key}", web::get().to(get_config))
                    .route("/config/{key}", web::put().to(set_config))
                    .route("/config/{key}", web::delete().to(delete_config))
                    .route(
                        "/config/{key}/action",
                        web::post().to(execute_config_action),
                    )
                    // audit logs
                    .route("/audit-logs", web::get().to(list_audit_logs))
                    // webdav locks
                    .route("/locks", web::get().to(list_locks))
                    .route("/locks/expired", web::delete().to(cleanup_expired_locks))
                    .route("/locks/{id}", web::delete().to(force_unlock)),
            ),
        )
}
