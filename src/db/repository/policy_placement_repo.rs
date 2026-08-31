//! Repository for storage placement profile rules and targets.

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{storage_policy_group_rule, storage_policy_group_rule_target};
use sea_orm::PaginatorTrait;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

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

pub async fn count_targets_by_rule_and_policy<C: ConnectionTrait>(
    db: &C,
    rule_id: i64,
    policy_id: i64,
) -> Result<u64> {
    storage_policy_group_rule_target::Entity::find()
        .filter(storage_policy_group_rule_target::Column::RuleId.eq(rule_id))
        .filter(storage_policy_group_rule_target::Column::PolicyId.eq(policy_id))
        .count(db)
        .await
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
