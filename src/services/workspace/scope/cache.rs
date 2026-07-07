//! 工作空间 scope 权限缓存。

use aster_forge_cache::CacheExt;
use crate::runtime::SharedRuntimeState;

use super::CachedTeamAccess;

const TEAM_ACCESS_CACHE_TTL: u64 = 60;
const ACTOR_USERNAME_CACHE_TTL: u64 = 60;

fn team_access_cache_prefix(team_id: i64) -> String {
    format!("team_access:{team_id}:")
}

fn team_access_cache_key(team_id: i64, user_id: i64) -> String {
    format!("{}{}", team_access_cache_prefix(team_id), user_id)
}

fn actor_username_cache_key(user_id: i64) -> String {
    format!("actor_username:{user_id}")
}

pub(super) async fn load_team_access(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Option<CachedTeamAccess> {
    state
        .cache()
        .get::<CachedTeamAccess>(&team_access_cache_key(team_id, user_id))
        .await
}

pub(super) async fn store_team_access(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
    access: &CachedTeamAccess,
) {
    state
        .cache()
        .set(
            &team_access_cache_key(team_id, user_id),
            access,
            Some(TEAM_ACCESS_CACHE_TTL),
        )
        .await;
}

pub(super) async fn invalidate_team_access_for_team(state: &impl SharedRuntimeState, team_id: i64) {
    state
        .cache()
        .invalidate_prefix(&team_access_cache_prefix(team_id))
        .await;
}

pub(super) async fn invalidate_team_access_for_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) {
    state
        .cache()
        .delete(&team_access_cache_key(team_id, user_id))
        .await;
}

pub(super) async fn load_actor_username(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Option<String> {
    state
        .cache()
        .get::<String>(&actor_username_cache_key(user_id))
        .await
}

pub(super) async fn store_actor_username(
    state: &impl SharedRuntimeState,
    user_id: i64,
    username: &str,
) {
    state
        .cache()
        .set(
            &actor_username_cache_key(user_id),
            &username,
            Some(ACTOR_USERNAME_CACHE_TTL),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;
    use crate::types::TeamMemberRole;

    fn access(team_id: i64, role: TeamMemberRole) -> CachedTeamAccess {
        CachedTeamAccess {
            team_id,
            policy_group_id: Some(team_id * 10),
            role,
        }
    }

    #[tokio::test]
    async fn team_access_member_invalidation_does_not_drop_other_members() {
        let state = CacheOnlyState::new().await;

        store_team_access(&state, 7, 10, &access(7, TeamMemberRole::Member)).await;
        store_team_access(&state, 7, 11, &access(7, TeamMemberRole::Admin)).await;

        invalidate_team_access_for_member(&state, 7, 10).await;

        assert!(load_team_access(&state, 7, 10).await.is_none());
        assert_eq!(
            load_team_access(&state, 7, 11)
                .await
                .map(|cached| cached.role),
            Some(TeamMemberRole::Admin)
        );
    }

    #[tokio::test]
    async fn team_access_team_invalidation_is_prefix_scoped() {
        let state = CacheOnlyState::new().await;

        store_team_access(&state, 7, 10, &access(7, TeamMemberRole::Member)).await;
        store_team_access(&state, 8, 10, &access(8, TeamMemberRole::Owner)).await;

        invalidate_team_access_for_team(&state, 7).await;

        assert!(load_team_access(&state, 7, 10).await.is_none());
        assert_eq!(
            load_team_access(&state, 8, 10)
                .await
                .map(|cached| cached.role),
            Some(TeamMemberRole::Owner)
        );
    }

    #[tokio::test]
    async fn actor_username_is_scoped_by_user_id() {
        let state = CacheOnlyState::new().await;

        store_actor_username(&state, 10, "alice").await;
        store_actor_username(&state, 11, "bob").await;

        assert_eq!(
            load_actor_username(&state, 10).await.as_deref(),
            Some("alice")
        );
        assert_eq!(
            load_actor_username(&state, 11).await.as_deref(),
            Some("bob")
        );
    }
}
