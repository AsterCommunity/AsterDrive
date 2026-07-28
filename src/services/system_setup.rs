//! Authoritative product setup state and transition guards.

use sea_orm::ConnectionTrait;
use serde::Serialize;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{policy_group_repo, policy_repo, user_repo};
use crate::errors::{AsterError, Result, validation_error_with_code};
use aster_drive_model::types::UserRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SystemSetupState {
    NeedsAdmin,
    NeedsStorage,
    Ready,
}

impl SystemSetupState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NeedsAdmin => "needs_admin",
            Self::NeedsStorage => "needs_storage",
            Self::Ready => "ready",
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSetupStatus {
    pub state: SystemSetupState,
    pub has_users: bool,
}

pub async fn inspect<C: ConnectionTrait>(db: &C) -> Result<SystemSetupStatus> {
    let user_count = user_repo::count_all(db).await?;
    if user_count == 0 {
        return Ok(SystemSetupStatus {
            state: SystemSetupState::NeedsAdmin,
            has_users: false,
        });
    }

    let admin_count = user_repo::count_by_role(db, UserRole::Admin).await?;
    if admin_count == 0 {
        return Err(AsterError::internal_error(
            "system contains users but no administrator; public setup remains closed",
        ));
    }

    let state = if configured_default_policy_group_id(db).await?.is_none()
        || user_repo::count_unassigned_by_role(db, UserRole::Admin).await? > 0
    {
        SystemSetupState::NeedsStorage
    } else {
        SystemSetupState::Ready
    };

    Ok(SystemSetupStatus {
        state,
        has_users: true,
    })
}

pub async fn state<C: ConnectionTrait>(db: &C) -> Result<SystemSetupState> {
    inspect(db).await.map(|status| status.state)
}

pub async fn require_ready<C: ConnectionTrait>(db: &C) -> Result<()> {
    match state(db).await? {
        SystemSetupState::Ready => Ok(()),
        SystemSetupState::NeedsAdmin => Err(validation_error_with_code(
            ApiErrorCode::ValidationSystemNotInitialized,
            "system is not initialized",
        )),
        SystemSetupState::NeedsStorage => Err(validation_error_with_code(
            ApiErrorCode::ValidationSystemNotInitialized,
            "system storage setup is incomplete",
        )),
    }
}

/// Requires that the one-time storage setup transition is still pending.
///
/// Call this after acquiring the setup lock and before creating the initial
/// default policy, so a concurrent request cannot create a second candidate.
pub async fn require_needs_storage<C: ConnectionTrait>(db: &C) -> Result<()> {
    match state(db).await? {
        SystemSetupState::NeedsStorage => Ok(()),
        SystemSetupState::NeedsAdmin => Err(validation_error_with_code(
            ApiErrorCode::ValidationSystemNotInitialized,
            "system administrator setup is incomplete",
        )),
        SystemSetupState::Ready => Err(validation_error_with_code(
            ApiErrorCode::ValidationSystemAlreadyInitialized,
            "system storage setup is already complete",
        )),
    }
}

pub async fn configured_default_policy_group_id<C: ConnectionTrait>(db: &C) -> Result<Option<i64>> {
    if policy_repo::find_default(db).await?.is_none() {
        return Ok(None);
    }

    let Some(group) = policy_group_repo::find_default_group(db).await? else {
        return Ok(None);
    };
    if !group.is_enabled
        || policy_group_repo::find_group_items(db, group.id)
            .await?
            .is_empty()
    {
        return Ok(None);
    }

    Ok(Some(group.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_migration::Migrator;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};

    use aster_drive_model::entities::{
        storage_policy, storage_policy_group, storage_policy_group_item, user,
    };
    use aster_drive_model::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserStatus,
    };

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("setup state test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("setup state test migrations should run");
        db
    }

    async fn insert_user(
        db: &sea_orm::DatabaseConnection,
        username: &str,
        role: UserRole,
        policy_group_id: Option<i64>,
    ) {
        let now = Utc::now();
        user::ActiveModel {
            username: Set(username.to_string()),
            email: Set(format!("{username}@example.com")),
            password_hash: Set("test-password-hash".to_string()),
            role: Set(role),
            status: Set(UserStatus::Active),
            must_change_password: Set(false),
            session_version: Set(1),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(policy_group_id),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("setup state test user should insert");
    }

    async fn insert_default_policy(db: &sea_orm::DatabaseConnection) -> i64 {
        let now = Utc::now();
        storage_policy::ActiveModel {
            name: Set("Shared Default".to_string()),
            driver_type: Set(DriverType::S3),
            endpoint: Set("http://storage.test".to_string()),
            bucket: Set("asterdrive".to_string()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(String::new()),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::empty()),
            is_default: Set(true),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("setup state test policy should insert")
        .id
    }

    async fn insert_default_group(
        db: &sea_orm::DatabaseConnection,
        policy_id: i64,
        is_enabled: bool,
        with_item: bool,
    ) -> i64 {
        let now = Utc::now();
        let group = storage_policy_group::ActiveModel {
            name: Set("Default Group".to_string()),
            description: Set(String::new()),
            is_enabled: Set(is_enabled),
            is_default: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("setup state test group should insert");
        if with_item {
            storage_policy_group_item::ActiveModel {
                group_id: Set(group.id),
                policy_id: Set(policy_id),
                priority: Set(1),
                min_file_size: Set(0),
                max_file_size: Set(0),
                created_at: Set(now),
                ..Default::default()
            }
            .insert(db)
            .await
            .expect("setup state test group item should insert");
        }
        group.id
    }

    #[tokio::test]
    async fn fresh_database_needs_admin() {
        let db = setup_db().await;

        assert_eq!(state(&db).await.unwrap(), SystemSetupState::NeedsAdmin);
        assert!(!inspect(&db).await.unwrap().has_users);
        assert_eq!(
            require_needs_storage(&db)
                .await
                .unwrap_err()
                .api_error_code(),
            ApiErrorCode::ValidationSystemNotInitialized
        );
    }

    #[tokio::test]
    async fn users_without_an_admin_do_not_reopen_public_setup() {
        let db = setup_db().await;
        insert_user(&db, "ordinary", UserRole::User, None).await;

        let error = state(&db).await.unwrap_err();
        assert!(error.to_string().contains("no administrator"));
    }

    #[tokio::test]
    async fn administrator_without_default_storage_needs_storage() {
        let db = setup_db().await;
        insert_user(&db, "admin", UserRole::Admin, None).await;

        assert_eq!(state(&db).await.unwrap(), SystemSetupState::NeedsStorage);
        require_needs_storage(&db).await.unwrap();
    }

    #[tokio::test]
    async fn incomplete_default_group_keeps_storage_setup_open() {
        let db = setup_db().await;
        let policy_id = insert_default_policy(&db).await;
        let disabled_group_id = insert_default_group(&db, policy_id, false, true).await;
        insert_user(&db, "admin", UserRole::Admin, Some(disabled_group_id)).await;

        assert_eq!(state(&db).await.unwrap(), SystemSetupState::NeedsStorage);
    }

    #[tokio::test]
    async fn empty_default_group_keeps_storage_setup_open() {
        let db = setup_db().await;
        let policy_id = insert_default_policy(&db).await;
        let group_id = insert_default_group(&db, policy_id, true, false).await;
        insert_user(&db, "admin", UserRole::Admin, Some(group_id)).await;

        assert_eq!(state(&db).await.unwrap(), SystemSetupState::NeedsStorage);
    }

    #[tokio::test]
    async fn unassigned_administrator_keeps_storage_setup_open() {
        let db = setup_db().await;
        let policy_id = insert_default_policy(&db).await;
        insert_default_group(&db, policy_id, true, true).await;
        insert_user(&db, "admin", UserRole::Admin, None).await;

        assert_eq!(state(&db).await.unwrap(), SystemSetupState::NeedsStorage);
    }

    #[tokio::test]
    async fn complete_default_storage_and_admin_assignment_are_ready() {
        let db = setup_db().await;
        let policy_id = insert_default_policy(&db).await;
        let group_id = insert_default_group(&db, policy_id, true, true).await;
        insert_user(&db, "admin", UserRole::Admin, Some(group_id)).await;

        assert_eq!(state(&db).await.unwrap(), SystemSetupState::Ready);
        require_ready(&db).await.unwrap();
        assert_eq!(
            require_needs_storage(&db)
                .await
                .unwrap_err()
                .api_error_code(),
            ApiErrorCode::ValidationSystemAlreadyInitialized
        );
    }
}
