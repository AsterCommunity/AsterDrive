//! 仓储模块：`property_repo`。

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, ExprTrait,
    FromQueryResult, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TryInsertResult,
    sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::entity_property::{self, Entity as EntityProperty};
use aster_drive_model::types::EntityType;

const ENTITY_PROPERTY_BATCH_CHUNK_SIZE: usize = 500;
pub(crate) const SYSTEM_PROPERTY_NAMESPACE_PREFIX: &str = "system.";
const DAV_PROPERTY_NAMESPACE: &str = "DAV:";

pub(crate) fn is_system_namespace(namespace: &str) -> bool {
    namespace.starts_with(SYSTEM_PROPERTY_NAMESPACE_PREFIX)
}

pub(crate) fn is_dav_namespace(namespace: &str) -> bool {
    namespace == DAV_PROPERTY_NAMESPACE
}

pub(crate) fn is_protected_namespace(namespace: &str) -> bool {
    is_dav_namespace(namespace) || is_system_namespace(namespace)
}

fn case_sensitive_column_eq(
    backend: DbBackend,
    column: entity_property::Column,
    value: &str,
) -> sea_orm::sea_query::SimpleExpr {
    let column = || Expr::col(column);
    let value = || Expr::val(value.to_owned());
    match backend {
        DbBackend::Sqlite => {
            Expr::cust_with_exprs("? COLLATE BINARY = ? COLLATE BINARY", [column(), value()])
        }
        DbBackend::MySql => Expr::cust_with_exprs("BINARY ? = BINARY ?", [column(), value()]),
        _ => column().eq(value()),
    }
}

/// Matches the persisted XML namespace identity with the same byte-sensitive
/// semantics as the unique key on every supported database backend.
pub(crate) fn namespace_eq_condition(
    backend: DbBackend,
    namespace: &str,
) -> sea_orm::sea_query::SimpleExpr {
    case_sensitive_column_eq(backend, entity_property::Column::Namespace, namespace)
}

fn property_key_condition(backend: DbBackend, namespace: &str, name: &str) -> sea_orm::Condition {
    sea_orm::Condition::all()
        .add(namespace_eq_condition(backend, namespace))
        .add(case_sensitive_column_eq(
            backend,
            entity_property::Column::Name,
            name,
        ))
}

pub(crate) fn user_namespace_condition(backend: DbBackend) -> sea_orm::Condition {
    let column = || Expr::col(entity_property::Column::Namespace);
    let exact_not_match = |value: &'static str| match backend {
        DbBackend::Sqlite => Expr::cust_with_exprs("NOT (? GLOB ?)", [column(), Expr::val(value)]),
        DbBackend::Postgres => column().ne(value),
        DbBackend::MySql => {
            Expr::cust_with_exprs("BINARY ? <> BINARY ?", [column(), Expr::val(value)])
        }
        _ => column().ne(value),
    };
    let prefix_not_match = match backend {
        DbBackend::Sqlite => Expr::cust_with_exprs(
            "NOT (? GLOB ?)",
            [
                column(),
                Expr::val(format!("{SYSTEM_PROPERTY_NAMESPACE_PREFIX}*")),
            ],
        ),
        DbBackend::Postgres => column().not_like(format!("{SYSTEM_PROPERTY_NAMESPACE_PREFIX}%")),
        DbBackend::MySql => Expr::cust_with_exprs(
            "BINARY ? NOT LIKE BINARY ?",
            [
                column(),
                Expr::val(format!("{SYSTEM_PROPERTY_NAMESPACE_PREFIX}%")),
            ],
        ),
        _ => column().not_like(format!("{SYSTEM_PROPERTY_NAMESPACE_PREFIX}%")),
    };

    // Namespace identifiers are case-sensitive. Backend-specific operators keep the
    // atomic DELETE aligned with the Rust predicate even under SQLite/MySQL defaults.
    sea_orm::Condition::all()
        .add(exact_not_match(DAV_PROPERTY_NAMESPACE))
        .add(prefix_not_match)
}

pub struct NewEntityProperty {
    pub entity_type: EntityType,
    pub entity_id: i64,
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
}

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

/// 查询多个实体的所有属性。
pub async fn find_by_entities<C: ConnectionTrait>(
    db: &C,
    targets: &[(EntityType, i64)],
) -> Result<Vec<entity_property::Model>> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut folders = Vec::new();
    for (entity_type, entity_id) in targets {
        match entity_type {
            EntityType::File => files.push(*entity_id),
            EntityType::Folder => folders.push(*entity_id),
        }
    }
    files.sort_unstable();
    files.dedup();
    folders.sort_unstable();
    folders.dedup();

    let mut props = Vec::new();
    for (entity_type, ids) in [(EntityType::File, files), (EntityType::Folder, folders)] {
        for chunk in ids.chunks(ENTITY_PROPERTY_BATCH_CHUNK_SIZE) {
            props.extend(
                EntityProperty::find()
                    .filter(entity_property::Column::EntityType.eq(entity_type))
                    .filter(entity_property::Column::EntityId.is_in(chunk.iter().copied()))
                    .order_by_asc(entity_property::Column::EntityType)
                    .order_by_asc(entity_property::Column::EntityId)
                    .order_by_asc(entity_property::Column::Namespace)
                    .order_by_asc(entity_property::Column::Name)
                    .all(db)
                    .await
                    .map_err(AsterError::from)?,
            );
        }
    }

    Ok(props)
}

/// 查询实体的单个属性
pub async fn find_by_key(
    db: &DatabaseConnection,
    entity_type: EntityType,
    entity_id: i64,
    namespace: &str,
    name: &str,
) -> Result<Option<entity_property::Model>> {
    EntityProperty::find()
        .filter(entity_property::Column::EntityType.eq(entity_type))
        .filter(entity_property::Column::EntityId.eq(entity_id))
        .filter(property_key_condition(
            db.get_database_backend(),
            namespace,
            name,
        ))
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
            .filter(property_key_condition(
                db.get_database_backend(),
                namespace,
                name,
            ))
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
        .filter(property_key_condition(
            db.get_database_backend(),
            namespace,
            name,
        ))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "entity property upsert could not reload row for {entity_type:?}#{entity_id} {namespace}:{name}"
            ))
        })
}

/// Inserts properties for newly-created entities in bounded batches.
pub async fn insert_many<C: ConnectionTrait>(
    db: &C,
    properties: Vec<NewEntityProperty>,
) -> Result<()> {
    for chunk in properties.chunks(ENTITY_PROPERTY_BATCH_CHUNK_SIZE) {
        EntityProperty::insert_many(chunk.iter().map(|property| entity_property::ActiveModel {
            entity_type: Set(property.entity_type),
            entity_id: Set(property.entity_id),
            namespace: Set(property.namespace.clone()),
            name: Set(property.name.clone()),
            value: Set(property.value.clone()),
            ..Default::default()
        }))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    }
    Ok(())
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
        .filter(property_key_condition(
            db.get_database_backend(),
            namespace,
            name,
        ))
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
            .filter(property_key_condition(
                db.get_database_backend(),
                namespace,
                name,
            ))
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
        .filter(property_key_condition(
            db.get_database_backend(),
            namespace,
            name,
        ))
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
        .filter(namespace_eq_condition(db.get_database_backend(), namespace))
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

pub async fn find_tag_bindings_for_entities(
    db: &DatabaseConnection,
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
        .filter(namespace_eq_condition(db.get_database_backend(), namespace))
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

pub async fn find_entity_ids_by_tag_ids(
    db: &DatabaseConnection,
    namespace: &str,
    entity_type: EntityType,
    tag_ids: &[i64],
) -> Result<Vec<i64>> {
    if tag_ids.is_empty() {
        return Ok(vec![]);
    }

    let tag_names = tag_ids.iter().map(i64::to_string).collect::<Vec<_>>();
    let rows = EntityProperty::find()
        .filter(namespace_eq_condition(db.get_database_backend(), namespace))
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

pub async fn count_entities_by_tag_ids(
    db: &DatabaseConnection,
    namespace: &str,
    tag_ids: &[i64],
) -> Result<std::collections::HashMap<i64, u64>> {
    if tag_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let tag_names = tag_ids.iter().map(i64::to_string).collect::<Vec<_>>();
    let rows = EntityProperty::find()
        .filter(namespace_eq_condition(db.get_database_backend(), namespace))
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

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, EntityTrait, QueryFilter, QueryTrait};

    use super::{
        EntityProperty, is_protected_namespace, namespace_eq_condition, property_key_condition,
        user_namespace_condition,
    };

    #[test]
    fn protected_namespace_matching_has_exact_dav_and_system_prefix_boundaries() {
        for namespace in ["DAV:", "system.", "system.preview"] {
            assert!(is_protected_namespace(namespace), "{namespace}");
        }
        for namespace in [
            "",
            "dav:",
            "DAV",
            "system",
            "System.preview",
            "systemx.preview",
            "urn:test",
        ] {
            assert!(!is_protected_namespace(namespace), "{namespace}");
        }
    }

    #[test]
    fn user_namespace_sql_uses_case_sensitive_backend_operators() {
        let sqlite = EntityProperty::find()
            .filter(user_namespace_condition(DbBackend::Sqlite))
            .build(DbBackend::Sqlite)
            .to_string();
        assert!(sqlite.contains("GLOB"), "{sqlite}");
        assert!(!sqlite.contains(" LIKE "), "{sqlite}");

        let postgres = EntityProperty::find()
            .filter(user_namespace_condition(DbBackend::Postgres))
            .build(DbBackend::Postgres)
            .to_string();
        assert!(postgres.contains("NOT LIKE"), "{postgres}");
        assert!(!postgres.contains("BINARY"), "{postgres}");

        let mysql = EntityProperty::find()
            .filter(user_namespace_condition(DbBackend::MySql))
            .build(DbBackend::MySql)
            .to_string();
        assert!(mysql.contains("BINARY"), "{mysql}");
        assert!(mysql.contains("NOT LIKE"), "{mysql}");
    }

    #[test]
    fn property_identity_sql_uses_case_sensitive_backend_operators() {
        for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
            let namespace = EntityProperty::find()
                .filter(namespace_eq_condition(backend, "System.preview"))
                .build(backend)
                .to_string();
            let key = EntityProperty::find()
                .filter(property_key_condition(backend, "System.preview", "Cache"))
                .build(backend)
                .to_string();

            match backend {
                DbBackend::Sqlite => {
                    assert!(namespace.contains("COLLATE BINARY"), "{namespace}");
                    assert_eq!(key.matches("COLLATE BINARY").count(), 4, "{key}");
                }
                DbBackend::Postgres => {
                    assert!(!namespace.contains("BINARY"), "{namespace}");
                    assert!(!key.contains("BINARY"), "{key}");
                }
                DbBackend::MySql => {
                    assert!(namespace.contains("BINARY"), "{namespace}");
                    assert_eq!(key.matches("BINARY").count(), 4, "{key}");
                }
                _ => unreachable!(),
            }
        }
    }
}
