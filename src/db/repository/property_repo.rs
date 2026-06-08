//! 仓储模块：`property_repo`。

use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter,
    QuerySelect, Set, TryInsertResult, sea_query::Expr,
};

use crate::entities::entity_property::{self, Entity as EntityProperty};
use crate::errors::{AsterError, Result};
use crate::types::EntityType;

const ENTITY_PROPERTY_BATCH_CHUNK_SIZE: usize = 500;

/// 查询实体的所有属性
pub async fn find_by_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<Vec<entity_property::Model>> {
    EntityProperty::find()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询实体的单个属性
pub async fn find_by_key<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    namespace: &str,
    name: &str,
) -> Result<Option<entity_property::Model>> {
    EntityProperty::find()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_property::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// 插入或更新属性
pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    namespace: &str,
    name: &str,
    value: Option<&str>,
) -> Result<entity_property::Model> {
    let value_owned = value.map(|v| v.to_string());
    let inserted = match EntityProperty::insert(entity_property::ActiveModel {
        entity_type: Set(entity_type),
        entity_id: Set(entity_id),
        namespace: Set(namespace.to_string()),
        name: Set(name.to_string()),
        value: Set(value_owned.clone()),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([
        entity_property::Column::EntityType,
        entity_property::Column::EntityId,
        entity_property::Column::Namespace,
        entity_property::Column::Name,
    ])
    .exec(db)
    .await
    .map_err(AsterError::from)?
    {
        TryInsertResult::Inserted(_) => true,
        TryInsertResult::Conflicted => false,
        TryInsertResult::Empty => {
            return Err(AsterError::internal_error(
                "entity property upsert produced empty insert result",
            ));
        }
    };

    if !inserted {
        let result = EntityProperty::update_many()
            .col_expr(
                entity_property::Column::Value,
                sea_orm::sea_query::Expr::value(value_owned.clone()),
            )
            .filter(entity_property::Column::EntityType.eq(entity_type))
            .filter(entity_property::Column::EntityId.eq(entity_id))
            .filter(entity_property::Column::Namespace.eq(namespace))
            .filter(entity_property::Column::Name.eq(name))
            .exec(db)
            .await
            .map_err(AsterError::from)?;

        if result.rows_affected == 0 {
            return Err(AsterError::internal_error(format!(
                "entity property upsert update affected 0 rows for {entity_type:?}#{entity_id} {namespace}:{name}"
            )));
        }
    }

    EntityProperty::find()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_property::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "entity property upsert could not reload row for {entity_type:?}#{entity_id} {namespace}:{name}"
            ))
        })
}

/// 删除单个属性
pub async fn delete_prop<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    namespace: &str,
    name: &str,
) -> Result<()> {
    EntityProperty::delete_many()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_property::Column::Name.eq(name))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量插入同一个属性到多个实体；已有属性保持不变。
pub async fn insert_many_for_entities<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_ids: &[i64],
    namespace: &str,
    name: &str,
    value: Option<&str>,
) -> Result<()> {
    if entity_ids.is_empty() {
        return Ok(());
    }

    let namespace = namespace.to_string();
    let name = name.to_string();
    let value = value.map(ToOwned::to_owned);

    for chunk in entity_ids.chunks(ENTITY_PROPERTY_BATCH_CHUNK_SIZE) {
        let models = chunk
            .iter()
            .map(|entity_id| entity_property::ActiveModel {
                entity_type: Set(entity_type),
                entity_id: Set(*entity_id),
                namespace: Set(namespace.clone()),
                name: Set(name.clone()),
                value: Set(value.clone()),
                ..Default::default()
            })
            .collect::<Vec<_>>();

        match EntityProperty::insert_many(models)
            .on_conflict_do_nothing_on([
                entity_property::Column::EntityType,
                entity_property::Column::EntityId,
                entity_property::Column::Namespace,
                entity_property::Column::Name,
            ])
            .exec(db)
            .await
            .map_err(AsterError::from)?
        {
            TryInsertResult::Inserted(_) | TryInsertResult::Conflicted => {}
            TryInsertResult::Empty => {
                return Err(AsterError::internal_error(
                    "entity property batch insert produced empty insert result",
                ));
            }
        }
    }

    Ok(())
}

/// 批量删除多个实体上的同一个属性。
pub async fn delete_many_for_entities<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_ids: &[i64],
    namespace: &str,
    name: &str,
) -> Result<()> {
    if entity_ids.is_empty() {
        return Ok(());
    }

    for chunk in entity_ids.chunks(ENTITY_PROPERTY_BATCH_CHUNK_SIZE) {
        EntityProperty::delete_many()
            .filter(entity_property::Column::EntityType.eq(entity_type))
            .filter(entity_property::Column::EntityId.is_in(chunk.iter().copied()))
            .filter(entity_property::Column::Namespace.eq(namespace))
            .filter(entity_property::Column::Name.eq(name))
            .exec(db)
            .await
            .map_err(AsterError::from)?;
    }

    Ok(())
}

/// 删除实体的所有属性（实体删除时级联清理）
pub async fn delete_all_for_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    EntityProperty::delete_many()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量删除多个实体的所有属性
pub async fn delete_all_for_entities<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_ids: &[i64],
) -> Result<()> {
    if entity_ids.is_empty() {
        return Ok(());
    }
    EntityProperty::delete_many()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.is_in(entity_ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 删除某个命名空间下指定属性名的所有绑定。
pub async fn delete_by_namespace_and_name<C: ConnectionTrait>(
    db: &C,
    namespace: &str,
    name: &str,
) -> Result<()> {
    EntityProperty::delete_many()
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_property::Column::Name.eq(name))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量删除某个实体在命名空间下的属性。
pub async fn delete_namespace_for_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    namespace: &str,
) -> Result<()> {
    EntityProperty::delete_many()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .filter(entity_property::Column::Namespace.eq(namespace))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量查找实体绑定的 tag id。
#[derive(Debug, FromQueryResult)]
pub struct EntityTagBindingRow {
    pub entity_type: EntityType,
    pub entity_id: i64,
    pub tag_id: String,
}

pub async fn find_tag_bindings_for_entities<C: ConnectionTrait>(
    db: &C,
    namespace: &str,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<Vec<EntityTagBindingRow>> {
    if file_ids.is_empty() && folder_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut entity_filter = sea_orm::Condition::any();
    if !file_ids.is_empty() {
        entity_filter = entity_filter.add(
            sea_orm::Condition::all()
                .add(entity_property::Column::EntityType.eq(EntityType::File))
                .add(entity_property::Column::EntityId.is_in(file_ids.iter().copied())),
        );
    }
    if !folder_ids.is_empty() {
        entity_filter = entity_filter.add(
            sea_orm::Condition::all()
                .add(entity_property::Column::EntityType.eq(EntityType::Folder))
                .add(entity_property::Column::EntityId.is_in(folder_ids.iter().copied())),
        );
    }

    EntityProperty::find()
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_filter)
        .select_only()
        .column(entity_property::Column::EntityType)
        .column(entity_property::Column::EntityId)
        .column_as(Expr::col(entity_property::Column::Name), "tag_id")
        .into_model::<EntityTagBindingRow>()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_entity_ids_by_tag_ids<C: ConnectionTrait>(
    db: &C,
    namespace: &str,
    entity_type: EntityType,
    tag_ids: &[i64],
) -> Result<Vec<i64>> {
    if tag_ids.is_empty() {
        return Ok(vec![]);
    }

    let tag_names = tag_ids.iter().map(i64::to_string).collect::<Vec<_>>();
    let rows = EntityProperty::find()
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::Name.is_in(tag_names))
        .select_only()
        .column(entity_property::Column::EntityId)
        .into_tuple::<i64>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok(rows)
}

pub async fn count_entities_by_tag_ids<C: ConnectionTrait>(
    db: &C,
    namespace: &str,
    tag_ids: &[i64],
) -> Result<std::collections::HashMap<i64, u64>> {
    if tag_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let tag_names = tag_ids.iter().map(i64::to_string).collect::<Vec<_>>();
    let rows = EntityProperty::find()
        .filter(entity_property::Column::Namespace.eq(namespace))
        .filter(entity_property::Column::Name.is_in(tag_names))
        .select_only()
        .column(entity_property::Column::Name)
        .column_as(entity_property::Column::Id.count(), "count")
        .group_by(entity_property::Column::Name)
        .into_tuple::<(String, i64)>()
        .all(db)
        .await
        .map_err(AsterError::from)?;

    let mut counts = std::collections::HashMap::with_capacity(rows.len());
    for (name, count) in rows {
        if let Ok(tag_id) = name.parse::<i64>() {
            let count = u64::try_from(count)
                .map_err(|_| AsterError::internal_error("negative tag binding count"))?;
            counts.insert(tag_id, count);
        }
    }
    Ok(counts)
}

/// 检查实体是否有自定义属性
pub async fn has_properties<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<bool> {
    let count = EntityProperty::find()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .count(db)
        .await
        .map_err(AsterError::from)?;
    Ok(count > 0)
}
