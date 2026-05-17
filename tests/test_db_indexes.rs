//! 集成测试：`db_indexes`。

#[macro_use]
mod common;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

async fn explain_query_plan(db: &DatabaseConnection, sql: &str) -> Vec<String> {
    db.query_all_raw(Statement::from_string(
        DbBackend::Sqlite,
        format!("EXPLAIN QUERY PLAN {sql}"),
    ))
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.try_get_by_index::<String>(3).unwrap())
    .collect()
}

fn skip_unless_sqlite(db: &DatabaseConnection) -> bool {
    db.get_database_backend() == DbBackend::Sqlite
}

fn assert_uses_index(plan: &[String], index: &str, table: &str) {
    assert!(
        plan.iter().any(|detail| detail.contains(index)),
        "expected planner to use {index}, got {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|detail| detail.contains(&format!("SCAN {table}"))),
        "expected planner to avoid scanning {table}, got {plan:?}"
    );
}

fn assert_no_temp_btree(plan: &[String]) {
    assert!(
        !plan
            .iter()
            .any(|detail| detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "expected planner to avoid temp ORDER BY b-tree, got {plan:?}"
    );
}

fn assert_uses_virtual_table(plan: &[String], virtual_table: &str, base_table: &str) {
    assert!(
        plan.iter().any(|detail| {
            detail.contains(virtual_table) && detail.contains("VIRTUAL TABLE INDEX")
        }),
        "expected planner to use virtual table {virtual_table}, got {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|detail| detail.starts_with(&format!("SCAN {base_table} "))),
        "expected planner to avoid full scanning {base_table}, got {plan:?}"
    );
}

#[actix_web::test]
async fn test_directory_lookup_indexes_cover_listing_and_duplicate_name_queries() {
    let state = common::setup().await;
    if !skip_unless_sqlite(&state.db) {
        return;
    }

    let folder_listing = explain_query_plan(
        &state.db,
        "SELECT * FROM folders \
         WHERE owner_user_id = 1 AND deleted_at IS NULL AND parent_id = 2 \
         ORDER BY name",
    )
    .await;
    assert_uses_index(
        &folder_listing,
        "idx_folders_owner_deleted_parent_name",
        "folders",
    );
    assert_no_temp_btree(&folder_listing);

    let file_listing = explain_query_plan(
        &state.db,
        "SELECT * FROM files \
         WHERE owner_user_id = 1 AND deleted_at IS NULL AND folder_id = 2 \
         ORDER BY name",
    )
    .await;
    assert_uses_index(
        &file_listing,
        "idx_files_owner_deleted_folder_name",
        "files",
    );
    assert_no_temp_btree(&file_listing);

    let folder_duplicate = explain_query_plan(
        &state.db,
        "SELECT * FROM folders \
         WHERE owner_user_id = 1 AND name = 'dup' AND deleted_at IS NULL AND parent_id = 2",
    )
    .await;
    assert_uses_index(
        &folder_duplicate,
        "idx_folders_owner_deleted_parent_name",
        "folders",
    );

    let file_duplicate = explain_query_plan(
        &state.db,
        "SELECT * FROM files \
         WHERE owner_user_id = 1 AND name = 'dup' AND deleted_at IS NULL AND folder_id = 2",
    )
    .await;
    assert_uses_index(
        &file_duplicate,
        "idx_files_owner_deleted_folder_name",
        "files",
    );
}

#[actix_web::test]
async fn test_trash_pagination_indexes_cover_deleted_item_queries() {
    let state = common::setup().await;
    if !skip_unless_sqlite(&state.db) {
        return;
    }

    let folder_trash = explain_query_plan(
        &state.db,
        "SELECT * FROM folders \
         WHERE owner_user_id = 1 \
           AND deleted_at IS NOT NULL \
           AND (parent_id IS NULL OR NOT EXISTS ( \
                SELECT 1 FROM folders p \
                WHERE p.id = folders.parent_id AND p.deleted_at IS NOT NULL \
           )) \
         ORDER BY deleted_at DESC \
         LIMIT 50 OFFSET 0",
    )
    .await;
    assert_uses_index(&folder_trash, "idx_folders_owner_deleted_at_id", "folders");
    assert_no_temp_btree(&folder_trash);

    let file_trash = explain_query_plan(
        &state.db,
        "SELECT * FROM files \
         WHERE owner_user_id = 1 \
           AND deleted_at IS NOT NULL \
           AND (folder_id IS NULL OR NOT EXISTS ( \
                SELECT 1 FROM folders f2 \
                WHERE f2.id = files.folder_id AND f2.deleted_at IS NOT NULL \
           )) \
         ORDER BY deleted_at DESC, id ASC \
         LIMIT 50",
    )
    .await;
    assert_uses_index(&file_trash, "idx_files_owner_deleted_at_id", "files");
    assert_no_temp_btree(&file_trash);
}

#[actix_web::test]
async fn test_sqlite_file_type_filter_indexes_cover_search_queries() {
    let state = common::setup().await;
    if !skip_unless_sqlite(&state.db) {
        return;
    }

    let personal_category = explain_query_plan(
        &state.db,
        "SELECT id FROM files \
         WHERE owner_user_id = 1 \
           AND team_id IS NULL \
           AND deleted_at IS NULL \
           AND file_category = 'image' \
           AND extension = 'jpg' \
         ORDER BY name \
         LIMIT 50",
    )
    .await;
    assert_uses_index(
        &personal_category,
        "idx_files_owner_deleted_category_ext",
        "files",
    );

    let personal_compound = explain_query_plan(
        &state.db,
        "SELECT id FROM files \
         WHERE owner_user_id = 1 \
           AND team_id IS NULL \
           AND deleted_at IS NULL \
           AND compound_extension = 'tar.gz' \
         LIMIT 50",
    )
    .await;
    assert_uses_index(
        &personal_compound,
        "idx_files_owner_deleted_compound_ext",
        "files",
    );

    let team_category = explain_query_plan(
        &state.db,
        "SELECT id FROM files \
         WHERE team_id = 7 \
           AND deleted_at IS NULL \
           AND file_category = 'video' \
           AND extension = 'mp4' \
         LIMIT 50",
    )
    .await;
    assert_uses_index(
        &team_category,
        "idx_files_team_deleted_category_ext",
        "files",
    );
}

#[actix_web::test]
async fn test_sqlite_search_fts_objects_exist() {
    let state = common::setup().await;
    if !skip_unless_sqlite(&state.db) {
        return;
    }

    let objects = state
        .db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_master \
             WHERE name IN (\
                 'files_name_fts', \
                 'folders_name_fts', \
                 'users_search_fts', \
                 'teams_search_fts', \
                 'trg_files_name_fts_ai', \
                 'trg_files_name_fts_ad', \
                 'trg_files_name_fts_au', \
                 'trg_folders_name_fts_ai', \
                 'trg_folders_name_fts_ad', \
                 'trg_folders_name_fts_au', \
                 'trg_users_search_fts_ai', \
                 'trg_users_search_fts_ad', \
                 'trg_users_search_fts_au', \
                 'trg_teams_search_fts_ai', \
                 'trg_teams_search_fts_ad', \
                 'trg_teams_search_fts_au'\
             )",
        ))
        .await
        .unwrap();

    let names: Vec<String> = objects
        .into_iter()
        .map(|row| row.try_get_by_index(0).unwrap())
        .collect();

    for expected in [
        "files_name_fts",
        "folders_name_fts",
        "users_search_fts",
        "teams_search_fts",
        "trg_files_name_fts_ai",
        "trg_files_name_fts_ad",
        "trg_files_name_fts_au",
        "trg_folders_name_fts_ai",
        "trg_folders_name_fts_ad",
        "trg_folders_name_fts_au",
        "trg_users_search_fts_ai",
        "trg_users_search_fts_ad",
        "trg_users_search_fts_au",
        "trg_teams_search_fts_ai",
        "trg_teams_search_fts_ad",
        "trg_teams_search_fts_au",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing sqlite search object {expected}: {names:?}"
        );
    }
}

#[actix_web::test]
async fn test_sqlite_search_fts_query_plan_uses_virtual_tables() {
    let state = common::setup().await;
    if !skip_unless_sqlite(&state.db) {
        return;
    }

    let file_search_count_plan = explain_query_plan(
        &state.db,
        "SELECT COUNT(*) \
         FROM files \
         WHERE files.deleted_at IS NULL \
           AND files.owner_user_id = 1 \
           AND files.team_id IS NULL \
           AND files.folder_id = 1 \
           AND files.id IN ( \
               SELECT rowid FROM files_name_fts WHERE files_name_fts MATCH '\"report\"' \
           )",
    )
    .await;
    assert_uses_virtual_table(&file_search_count_plan, "files_name_fts", "files");

    let file_search_plan = explain_query_plan(
        &state.db,
        "SELECT \
             files.id, files.name, file_blobs.size \
         FROM files \
         JOIN file_blobs ON file_blobs.id = files.blob_id \
         WHERE files.deleted_at IS NULL \
           AND files.owner_user_id = 1 \
           AND files.team_id IS NULL \
           AND files.folder_id = 1 \
           AND files.id IN ( \
               SELECT rowid FROM files_name_fts WHERE files_name_fts MATCH '\"report\"' \
           ) \
         ORDER BY files.name ASC \
         LIMIT 50 OFFSET 0",
    )
    .await;
    assert_uses_virtual_table(&file_search_plan, "files_name_fts", "files");

    let folder_search_plan = explain_query_plan(
        &state.db,
        "SELECT \
             folders.id, folders.name \
         FROM folders \
         WHERE folders.deleted_at IS NULL \
           AND folders.owner_user_id = 1 \
           AND folders.team_id IS NULL \
           AND folders.id IN ( \
               SELECT rowid FROM folders_name_fts WHERE folders_name_fts MATCH '\"docs\"' \
           ) \
         ORDER BY folders.name ASC \
         LIMIT 50 OFFSET 0",
    )
    .await;
    assert_uses_virtual_table(&folder_search_plan, "folders_name_fts", "folders");

    let user_search_plan = explain_query_plan(
        &state.db,
        "SELECT users.* \
         FROM users \
         WHERE users.id IN ( \
             SELECT rowid FROM users_search_fts WHERE users_search_fts MATCH '\"alice\"' \
         ) \
           AND users.role = 'user' \
           AND users.status = 'active' \
         ORDER BY users.id ASC \
         LIMIT 50 OFFSET 0",
    )
    .await;
    assert_uses_virtual_table(&user_search_plan, "users_search_fts", "users");

    let team_search_plan = explain_query_plan(
        &state.db,
        "SELECT teams.* \
         FROM teams \
         WHERE teams.archived_at IS NULL \
           AND teams.id IN ( \
               SELECT rowid FROM teams_search_fts WHERE teams_search_fts MATCH '\"oper\"' \
           ) \
         ORDER BY teams.id ASC \
         LIMIT 50 OFFSET 0",
    )
    .await;
    assert_uses_virtual_table(&team_search_plan, "teams_search_fts", "teams");

    let team_member_search_plan = explain_query_plan(
        &state.db,
        "SELECT team_members.id, users.username \
         FROM team_members \
         JOIN users ON users.id = team_members.user_id \
         WHERE team_members.team_id = 1 \
           AND users.id IN ( \
               SELECT rowid FROM users_search_fts WHERE users_search_fts MATCH '\"alice\"' \
           ) \
         ORDER BY users.username ASC \
         LIMIT 50 OFFSET 0",
    )
    .await;
    assert_uses_virtual_table(&team_member_search_plan, "users_search_fts", "users");
}
