//! `folder_repo` 仓储聚合入口。

mod common;
mod mutation;
mod path;
mod query;
mod trash;

pub(crate) use common::FolderScope;
pub use common::{
    duplicate_name_error, duplicate_name_message, is_any_duplicate_name_error,
    is_duplicate_name_error, is_name_conflict_db_err, map_bulk_name_db_err, map_name_db_err,
};
pub use mutation::{
    clear_policy_references, create, create_many, delete, delete_many, move_many_to_parent,
};
pub(crate) use path::{find_ancestor_models, find_team_ancestor_models};
pub use path::{find_ancestors, resolve_path_chain};
pub(crate) use query::find_child_ids_in_parents;
pub use query::{
    find_all_children, find_all_children_in_parents, find_all_files_in_folder, find_by_id,
    find_by_ids, find_by_ids_in_personal_scope, find_by_ids_in_team_scope, find_by_name_in_parent,
    find_by_name_in_team_parent, find_children, find_children_in_parents, find_children_paginated,
    find_team_children, find_team_children_in_parents, find_team_children_paginated, lock_by_id,
};
pub use trash::{
    find_all_by_team, find_all_by_team_paginated, find_all_by_user, find_all_by_user_paginated,
    find_deleted_by_user, find_deleted_children, find_expired_deleted,
    find_top_level_deleted_by_team_cursor, find_top_level_deleted_by_team_paginated,
    find_top_level_deleted_cursor, find_top_level_deleted_paginated, restore, restore_many,
    soft_delete, soft_delete_many,
};
