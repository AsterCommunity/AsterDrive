//! SeaORM entity for staged folder-tree mutation membership.

use sea_orm::entity::prelude::*;

use crate::types::EntityType;

/// Staged membership of one bounded folder-tree mutation (trash, restore,
/// delete). Rows are written while the task holds its serialization lock and
/// applied in bulk before the task finishes, then cleared with the task.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "folder_tree_operation_members")]
pub struct Model {
    /// Background task this staging batch belongs to; part of the composite
    /// primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub task_id: i64,
    /// Entity kind staged for the batch (`file` or `folder`); part of the
    /// composite primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub resource_kind: EntityType,
    /// Entity id staged for the batch; unique per `(task_id, resource_kind)`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub resource_id: i64,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The background task owning this staging batch (FK `task_id`, cascading).
    #[sea_orm(
        belongs_to = "super::background_task::Entity",
        from = "Column::TaskId",
        to = "super::background_task::Column::Id",
        on_delete = "Cascade",
        on_update = "Cascade"
    )]
    BackgroundTask,
}

impl Related<super::background_task::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BackgroundTask.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::RelationType;

    #[test]
    fn background_task_relation_uses_task_id_and_cascades() {
        let relation = <Entity as Related<super::super::background_task::Entity>>::to();

        assert_eq!(relation.rel_type, RelationType::HasOne);
        assert!(!relation.skip_fk);
        assert_eq!(format!("{:?}", relation.on_delete), "Some(Cascade)");
        assert_eq!(format!("{:?}", relation.on_update), "Some(Cascade)");
    }
}
