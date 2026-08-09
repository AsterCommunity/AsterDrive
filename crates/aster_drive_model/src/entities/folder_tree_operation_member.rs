//! SeaORM entity for staged folder-tree mutation membership.

use sea_orm::entity::prelude::*;

use crate::types::EntityType;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "folder_tree_operation_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub task_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub resource_kind: EntityType,
    #[sea_orm(primary_key, auto_increment = false)]
    pub resource_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
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
