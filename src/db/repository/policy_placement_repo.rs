//! Repository for storage placement profile rules and targets.

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{storage_policy_group_rule, storage_policy_group_rule_target};
use sea_orm::PaginatorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect,
    RelationTrait,
};

pub async fn find_all_rules<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_group_rule::Model>> {
    storage_policy_group_rule::Entity::find()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_targets<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_group_rule_target::Model>> {
    storage_policy_group_rule_target::Entity::find()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn group_has_rules<C: ConnectionTrait>(db: &C, group_id: i64) -> Result<bool> {
    storage_policy_group_rule::Entity::find()
        .filter(storage_policy_group_rule::Column::GroupId.eq(group_id))
        .one(db)
        .await
        .map(|rule| rule.is_some())
        .map_err(AsterError::from)
}

pub async fn find_rules_by_group_id<C: ConnectionTrait>(
    db: &C,
    group_id: i64,
) -> Result<Vec<storage_policy_group_rule::Model>> {
    storage_policy_group_rule::Entity::find()
        .filter(storage_policy_group_rule::Column::GroupId.eq(group_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_targets_by_rule_id<C: ConnectionTrait>(
    db: &C,
    rule_id: i64,
) -> Result<Vec<storage_policy_group_rule_target::Model>> {
    storage_policy_group_rule_target::Entity::find()
        .filter(storage_policy_group_rule_target::Column::RuleId.eq(rule_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn group_has_assignable_target<C: ConnectionTrait>(
    db: &C,
    group_id: i64,
) -> Result<bool> {
    use sea_orm::JoinType;

    storage_policy_group_rule_target::Entity::find()
        .join(
            JoinType::InnerJoin,
            storage_policy_group_rule_target::Relation::Rule.def(),
        )
        .join(
            JoinType::InnerJoin,
            storage_policy_group_rule_target::Relation::StoragePolicy.def(),
        )
        .filter(storage_policy_group_rule::Column::GroupId.eq(group_id))
        .filter(storage_policy_group_rule::Column::IsEnabled.eq(true))
        .filter(storage_policy_group_rule_target::Column::IsEnabled.eq(true))
        .filter(storage_policy_group_rule_target::Column::AcceptingNewWrites.eq(true))
        .one(db)
        .await
        .map(|target| target.is_some())
        .map_err(AsterError::from)
}

pub async fn create_rule<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group_rule::ActiveModel,
) -> Result<storage_policy_group_rule::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn create_target<C: ConnectionTrait>(
    db: &C,
    model: storage_policy_group_rule_target::ActiveModel,
) -> Result<storage_policy_group_rule_target::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn delete_rules_by_group<C: ConnectionTrait>(db: &C, group_id: i64) -> Result<u64> {
    let result = storage_policy_group_rule::Entity::delete_many()
        .filter(storage_policy_group_rule::Column::GroupId.eq(group_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn count_targets_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<u64> {
    storage_policy_group_rule_target::Entity::find()
        .filter(storage_policy_group_rule_target::Column::PolicyId.eq(policy_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sea_orm::{DbBackend, MockDatabase, MockExecResult};

    fn rule(id: i64, group_id: i64) -> storage_policy_group_rule::Model {
        let now = Utc::now();
        storage_policy_group_rule::Model {
            id,
            group_id,
            name: format!("rule-{id}"),
            description: String::new(),
            priority: 1,
            is_enabled: true,
            matcher: "{}".to_string(),
            selection_mode: "first_available".to_string(),
            unavailable_behavior: "next_rule".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    fn target(id: i64, rule_id: i64, policy_id: i64) -> storage_policy_group_rule_target::Model {
        let now = Utc::now();
        storage_policy_group_rule_target::Model {
            id,
            rule_id,
            policy_id,
            weight: 100,
            is_enabled: true,
            accepting_new_writes: true,
            stable_order: 1,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn placement_queries_return_scoped_rules_and_targets() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![rule(1, 7)]])
            .append_query_results([vec![target(2, 1, 9)]])
            .append_query_results([vec![rule(1, 7)]])
            .append_query_results([vec![rule(1, 7)]])
            .append_query_results([vec![target(2, 1, 9)]])
            .into_connection();

        assert_eq!(find_all_rules(&db).await.unwrap().len(), 1);
        assert_eq!(find_all_targets(&db).await.unwrap().len(), 1);
        assert!(group_has_rules(&db, 7).await.unwrap());
        assert_eq!(find_rules_by_group_id(&db, 7).await.unwrap()[0].id, 1);
        assert_eq!(find_targets_by_rule_id(&db, 1).await.unwrap()[0].id, 2);

        let log = db.into_transaction_log();
        let sql = log
            .iter()
            .flat_map(|transaction| transaction.statements().iter().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(sql.contains("storage_policy_group_rules"));
        assert!(sql.contains("storage_policy_group_rule_targets"));
        assert!(sql.contains("group_id"));
        assert!(sql.contains("rule_id"));
    }

    #[tokio::test]
    async fn assignable_target_query_and_rule_delete_preserve_repository_contract() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![target(2, 1, 9)]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 3,
            }])
            .into_connection();

        assert!(group_has_assignable_target(&db, 7).await.unwrap());
        assert_eq!(delete_rules_by_group(&db, 7).await.unwrap(), 3);

        let sql = db
            .into_transaction_log()
            .iter()
            .map(|transaction| format!("{transaction:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(sql.contains("accepting_new_writes"));
        assert!(sql.contains("is_enabled"));
        assert!(sql.contains("DELETE FROM"));
    }
}
