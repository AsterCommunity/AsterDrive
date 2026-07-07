//! 集成测试：`webdav_path_resolver`。

#[macro_use]
mod common;

use std::time::Duration;

use aster_drive::runtime::SharedRuntimeState;
use aster_drive::webdav::dav::{DavFileSystem, DavPath, FsError, OpenOptions, ReadDirMeta};
use bytes::Bytes;
use futures::StreamExt;
use sea_orm::{ActiveModelTrait, Set};

fn write_temp_fixture(name: &str, contents: &str) -> String {
    let dir = format!("/tmp/asterdrive-webdav-path-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/{name}");
    std::fs::write(&path, contents).unwrap();
    path
}

fn write_open_options(create_new: bool) -> OpenOptions {
    OpenOptions {
        read: false,
        write: true,
        append: false,
        truncate: false,
        create: false,
        create_new,
        size: None,
        checksum: None,
    }
}

async fn recv_storage_event(
    rx: &mut tokio::sync::broadcast::Receiver<
        aster_drive::services::storage_change_service::StorageChangeEvent,
    >,
) -> aster_drive::services::storage_change_service::StorageChangeEvent {
    tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("should receive storage change event")
        .expect("storage change channel should stay open")
}

async fn seed_nested_file(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    root_parent_id: Option<i64>,
) -> (
    aster_drive::services::workspace::models::FolderInfo,
    aster_drive::services::workspace::models::FolderInfo,
    aster_drive::services::workspace::models::FolderInfo,
    aster_drive::services::workspace::models::FileInfo,
    String,
) {
    use aster_drive::services::{files::file, files::folder};

    let projects = folder::create(state, user_id, "projects", root_parent_id)
        .await
        .unwrap();
    let docs = folder::create(state, user_id, "docs", Some(projects.id))
        .await
        .unwrap();
    let reports = folder::create(state, user_id, "reports", Some(docs.id))
        .await
        .unwrap();

    let contents = "deep path contents".to_string();
    let temp_path = write_temp_fixture("q1.txt", &contents);
    let file = file::store_from_temp(
        state,
        user_id,
        file::StoreFromTempRequest::new(
            Some(reports.id),
            "q1.txt",
            &temp_path,
            contents.len() as i64,
        ),
    )
    .await
    .unwrap();

    (projects, docs, reports, file, contents)
}

#[actix_web::test]
async fn test_path_resolver_resolves_deep_folder_file_and_parent_paths() {
    use aster_drive::services::auth::local;
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_parent, resolve_path};

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davresolverdeep",
        "davresolverdeep@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let (_projects, _docs, reports, file, _contents) =
        seed_nested_file(&state, user.id, None).await;

    let folder_path = DavPath::new("/projects/docs/reports").unwrap();
    match resolve_path(state.writer_db(), user.id, &folder_path, None)
        .await
        .unwrap()
    {
        ResolvedNode::Folder(folder) => assert_eq!(folder.id, reports.id),
        other => panic!("expected folder, got {other:?}"),
    }

    let file_path = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    match resolve_path(state.writer_db(), user.id, &file_path, None)
        .await
        .unwrap()
    {
        ResolvedNode::File(found) => assert_eq!(found.id, file.id),
        other => panic!("expected file, got {other:?}"),
    }

    let missing_intermediate = DavPath::new("/projects/docs/q1.txt/final.txt").unwrap();
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &missing_intermediate, None).await,
        Err(FsError::NotFound)
    ));

    let (parent_id, leaf_name) = resolve_parent(state.writer_db(), user.id, &file_path, None)
        .await
        .unwrap();
    assert_eq!(parent_id, Some(reports.id));
    assert_eq!(leaf_name, "q1.txt");
}

#[actix_web::test]
async fn test_path_resolver_uses_canonical_dav_path_segments() {
    use aster_drive::services::auth::local;
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_path};

    let state = common::setup().await;
    let user = local::register(&state, "davcanon", "davcanon@example.com", "pass1234")
        .await
        .unwrap();

    let (_projects, _docs, _reports, file, _contents) =
        seed_nested_file(&state, user.id, None).await;

    let canonical = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    let dot_alias = DavPath::new("/projects/./docs/reports/q1.txt").unwrap();
    let parent_alias = DavPath::new("/projects/docs/archive/../reports/q1.txt").unwrap();
    let encoded_parent_alias =
        DavPath::new("/projects/docs/archive/%2e%2e/reports/q1.txt").unwrap();

    assert_eq!(dot_alias.as_bytes(), canonical.as_bytes());
    assert_eq!(parent_alias.as_bytes(), canonical.as_bytes());
    assert_eq!(encoded_parent_alias.as_bytes(), canonical.as_bytes());

    for path in [canonical, dot_alias, parent_alias, encoded_parent_alias] {
        match resolve_path(state.writer_db(), user.id, &path, None)
            .await
            .unwrap()
        {
            ResolvedNode::File(found) => assert_eq!(found.id, file.id),
            other => panic!("expected canonicalized file path, got {other:?}"),
        }
    }
}

#[actix_web::test]
async fn test_path_resolver_honors_scoped_root_semantics() {
    use aster_drive::services::{auth::local, files::folder};
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_parent, resolve_path};

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davresolverscope",
        "davresolverscope@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let scoped_root = folder::create(&state, user.id, "scoped-root", None)
        .await
        .unwrap();
    let (_projects, _docs, _reports, file, _contents) =
        seed_nested_file(&state, user.id, Some(scoped_root.id)).await;

    let root_path = DavPath::new("/").unwrap();
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &root_path, Some(scoped_root.id))
            .await
            .unwrap(),
        ResolvedNode::Root
    ));

    let scoped_file_path = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    match resolve_path(
        state.writer_db(),
        user.id,
        &scoped_file_path,
        Some(scoped_root.id),
    )
    .await
    .unwrap()
    {
        ResolvedNode::File(found) => assert_eq!(found.id, file.id),
        other => panic!("expected scoped file, got {other:?}"),
    }

    let escaped_scope = DavPath::new("/scoped-root/projects/docs/reports/q1.txt").unwrap();
    assert!(matches!(
        resolve_path(
            state.writer_db(),
            user.id,
            &escaped_scope,
            Some(scoped_root.id)
        )
        .await,
        Err(FsError::NotFound)
    ));

    let (parent_id, leaf_name) = resolve_parent(
        state.writer_db(),
        user.id,
        &DavPath::new("/draft.txt").unwrap(),
        Some(scoped_root.id),
    )
    .await
    .unwrap();
    assert_eq!(parent_id, Some(scoped_root.id));
    assert_eq!(leaf_name, "draft.txt");
}

#[actix_web::test]
async fn test_path_resolver_handles_root_level_and_missing_path_boundaries() {
    use aster_drive::services::{auth::local, files::file};
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_parent, resolve_path};

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davresolverroot",
        "davresolverroot@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let contents = "root level file";
    let temp_path = write_temp_fixture("root.txt", contents);
    let file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(None, "root.txt", &temp_path, contents.len() as i64),
    )
    .await
    .unwrap();

    assert!(matches!(
        resolve_path(
            state.writer_db(),
            user.id,
            &DavPath::new("/").unwrap(),
            None
        )
        .await
        .unwrap(),
        ResolvedNode::Root
    ));
    assert!(matches!(
        resolve_parent(
            state.writer_db(),
            user.id,
            &DavPath::new("/").unwrap(),
            None
        )
        .await,
        Err(FsError::Forbidden)
    ));

    match resolve_path(
        state.writer_db(),
        user.id,
        &DavPath::new("/root.txt").unwrap(),
        None,
    )
    .await
    .unwrap()
    {
        ResolvedNode::File(found) => assert_eq!(found.id, file.id),
        other => panic!("expected root-level file, got {other:?}"),
    }

    assert!(matches!(
        resolve_path(
            state.writer_db(),
            user.id,
            &DavPath::new("/missing.txt").unwrap(),
            None
        )
        .await,
        Err(FsError::NotFound)
    ));
    assert!(matches!(
        resolve_path(
            state.writer_db(),
            user.id,
            &DavPath::new("/missing/final.txt").unwrap(),
            None,
        )
        .await,
        Err(FsError::NotFound)
    ));
    assert!(matches!(
        resolve_parent(
            state.writer_db(),
            user.id,
            &DavPath::new("/missing/final.txt").unwrap(),
            None,
        )
        .await,
        Err(FsError::NotFound)
    ));
}

#[actix_web::test]
async fn test_path_resolver_prefers_folder_when_file_and_folder_share_name() {
    use aster_drive::services::{auth::local, files::file, files::folder};
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_parent, resolve_path};

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davresolverdupe",
        "davresolverdupe@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let folder = folder::create(&state, user.id, "shared-name", None)
        .await
        .unwrap();
    let temp_path = write_temp_fixture("shared-name", "same name as folder");
    let file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(None, "shared-name", &temp_path, 19),
    )
    .await
    .unwrap();

    match resolve_path(
        state.writer_db(),
        user.id,
        &DavPath::new("/shared-name").unwrap(),
        None,
    )
    .await
    .unwrap()
    {
        ResolvedNode::Folder(found) => assert_eq!(found.id, folder.id),
        other => panic!("expected folder precedence, got {other:?}"),
    }

    let (parent_id, leaf_name) = resolve_parent(
        state.writer_db(),
        user.id,
        &DavPath::new("/shared-name/child.txt").unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(parent_id, Some(folder.id));
    assert_eq!(leaf_name, "child.txt");

    let root_level_file = resolve_path(
        state.writer_db(),
        user.id,
        &DavPath::new("/shared-name").unwrap(),
        None,
    )
    .await
    .unwrap();
    assert!(!matches!(root_level_file, ResolvedNode::File(found) if found.id == file.id));
}

#[actix_web::test]
async fn test_path_resolver_hides_deleted_intermediate_folders() {
    use aster_drive::services::auth::local;
    use aster_drive::webdav::path_resolver::{resolve_parent, resolve_path};

    let state = common::setup().await;
    let user = local::register(&state, "davresdel", "davresdel@example.com", "pass1234")
        .await
        .unwrap();

    let (_projects, docs, _reports, _file, _contents) =
        seed_nested_file(&state, user.id, None).await;

    let docs_model =
        aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), docs.id)
            .await
            .unwrap();
    let mut deleted_docs: aster_drive::entities::folder::ActiveModel = docs_model.into();
    deleted_docs.deleted_at = Set(Some(chrono::Utc::now()));
    deleted_docs.updated_at = Set(chrono::Utc::now());
    deleted_docs.update(state.writer_db()).await.unwrap();

    let file_path = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &file_path, None).await,
        Err(FsError::NotFound)
    ));
    assert!(matches!(
        resolve_parent(state.writer_db(), user.id, &file_path, None).await,
        Err(FsError::NotFound)
    ));
}

#[actix_web::test]
async fn test_cached_path_resolver_rejects_stale_paths_after_ancestor_rename() {
    use aster_drive::services::{auth::local, files::folder};
    use aster_drive::types::NullablePatch;
    use aster_drive::webdav::path_resolver::{
        ResolvedNode, resolve_parent_cached, resolve_path_cached,
    };

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davresolverstale",
        "davresolverstale@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let (_projects, docs, reports, file, _contents) = seed_nested_file(&state, user.id, None).await;
    let file_path = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    let new_file_path = DavPath::new("/projects/manuals/reports/q1.txt").unwrap();
    let pending_create_path = DavPath::new("/projects/docs/reports/new.txt").unwrap();

    match resolve_path_cached(&state, user.id, &file_path, None)
        .await
        .unwrap()
    {
        ResolvedNode::File(found) => assert_eq!(found.id, file.id),
        other => panic!("expected cached file, got {other:?}"),
    }
    let (cached_parent_id, cached_leaf) =
        resolve_parent_cached(&state, user.id, &pending_create_path, None)
            .await
            .unwrap();
    assert_eq!(cached_parent_id, Some(reports.id));
    assert_eq!(cached_leaf, "new.txt");

    folder::update(
        &state,
        docs.id,
        user.id,
        Some("manuals".to_string()),
        NullablePatch::Absent,
        NullablePatch::Absent,
    )
    .await
    .unwrap();

    assert!(matches!(
        resolve_path_cached(&state, user.id, &file_path, None).await,
        Err(FsError::NotFound)
    ));
    assert!(matches!(
        resolve_parent_cached(&state, user.id, &pending_create_path, None).await,
        Err(FsError::NotFound)
    ));

    match resolve_path_cached(&state, user.id, &new_file_path, None)
        .await
        .unwrap()
    {
        ResolvedNode::File(found) => assert_eq!(found.id, file.id),
        other => panic!("expected file at renamed ancestor path, got {other:?}"),
    }
}

#[actix_web::test]
async fn test_aster_dav_fs_handles_deep_paths_inside_scoped_root() {
    use aster_drive::services::{auth::local, files::folder};
    use aster_drive::webdav::fs::AsterDavFs;

    let state = common::setup().await;
    let user = local::register(&state, "davfsdeep", "davfsdeep@example.com", "pass1234")
        .await
        .unwrap();

    let scoped_root = folder::create(&state, user.id, "scoped-root", None)
        .await
        .unwrap();
    let (_projects, _docs, _reports, _file, contents) =
        seed_nested_file(&state, user.id, Some(scoped_root.id)).await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, Some(scoped_root.id));

    let root_path = DavPath::new("/").unwrap();
    let mut root_entries = dav_fs
        .read_dir(&root_path, ReadDirMeta::Data)
        .await
        .unwrap();
    let mut root_names = Vec::new();
    while let Some(entry) = root_entries.next().await {
        root_names.push(String::from_utf8(entry.unwrap().name()).unwrap());
    }
    assert_eq!(root_names, vec!["projects"]);

    let file_path = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    let metadata = dav_fs.metadata(&file_path).await.unwrap();
    assert_eq!(metadata.len(), contents.len() as u64);
    assert!(!metadata.is_dir());

    let dir_path = DavPath::new("/projects/docs/reports").unwrap();
    let mut dir_entries = dav_fs.read_dir(&dir_path, ReadDirMeta::Data).await.unwrap();
    let mut dir_names = Vec::new();
    while let Some(entry) = dir_entries.next().await {
        dir_names.push(String::from_utf8(entry.unwrap().name()).unwrap());
    }
    assert_eq!(dir_names, vec!["q1.txt"]);

    assert!(matches!(
        dav_fs.open(&file_path, OpenOptions::read()).await,
        Err(FsError::Forbidden)
    ));
}

#[actix_web::test]
async fn test_aster_dav_fs_deep_write_create_new_and_overwrite_boundaries() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::{auth::local, files::folder};
    use aster_drive::webdav::fs::AsterDavFs;

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davfswriteroot",
        "davfswriteroot@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let scoped_root = folder::create(&state, user.id, "scoped-root", None)
        .await
        .unwrap();
    let (projects, docs, reports, _file, _contents) =
        seed_nested_file(&state, user.id, Some(scoped_root.id)).await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, Some(scoped_root.id));

    let new_file_path = DavPath::new("/projects/docs/reports/new.txt").unwrap();
    let mut new_file = dav_fs
        .open(&new_file_path, write_open_options(true))
        .await
        .unwrap();
    new_file
        .write_bytes(Bytes::from_static(b"first version"))
        .await
        .unwrap();
    new_file.flush().await.unwrap();

    let stored =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, Some(reports.id), "new.txt")
            .await
            .unwrap()
            .expect("deep WebDAV write should create a file");
    assert_eq!(stored.size, "first version".len() as i64);

    assert!(matches!(
        dav_fs.open(&new_file_path, write_open_options(true)).await,
        Err(FsError::Exists)
    ));

    let mut overwrite = dav_fs
        .open(&new_file_path, write_open_options(false))
        .await
        .unwrap();
    overwrite
        .write_bytes(Bytes::from_static(b"updated"))
        .await
        .unwrap();
    overwrite.flush().await.unwrap();

    let overwritten =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, Some(reports.id), "new.txt")
            .await
            .unwrap()
            .expect("overwritten file should still exist");
    assert_eq!(overwritten.size, "updated".len() as i64);

    assert!(matches!(
        dav_fs
            .open(
                &DavPath::new("/projects/missing/new.txt").unwrap(),
                write_open_options(false),
            )
            .await,
        Err(FsError::NotFound)
    ));

    let parent_names = [projects.name, docs.name, reports.name];
    assert_eq!(parent_names, ["projects", "docs", "reports"]);
}

#[actix_web::test]
async fn test_aster_dav_fs_copy_file_publishes_storage_event() {
    use aster_drive::services::auth::local;
    use aster_drive::services::storage_change_service::{
        StorageChangeKind, StorageChangeWorkspace,
    };
    use aster_drive::webdav::fs::AsterDavFs;
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_path};

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davfscopyfile",
        "davfscopyfile@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let (_projects, _docs, reports, file, _contents) =
        seed_nested_file(&state, user.id, None).await;

    let mut rx = state.storage_change_tx.subscribe();
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);

    let source = DavPath::new("/projects/docs/reports/q1.txt").unwrap();
    let destination = DavPath::new("/projects/docs/reports/q1-copy.txt").unwrap();
    dav_fs.copy(&source, &destination).await.unwrap();

    let event = recv_storage_event(&mut rx).await;
    assert_eq!(event.kind, StorageChangeKind::FileCreated);
    assert!(matches!(
        event.workspace,
        Some(StorageChangeWorkspace::Personal)
    ));
    assert_eq!(event.file_ids.len(), 1);
    assert!(event.folder_ids.is_empty());
    assert_eq!(event.affected_parent_ids, vec![reports.id]);
    assert!(!event.root_affected);

    match resolve_path(state.writer_db(), user.id, &destination, None)
        .await
        .unwrap()
    {
        ResolvedNode::File(found) => assert_ne!(found.id, file.id),
        other => panic!("expected copied file, got {other:?}"),
    }
}

#[actix_web::test]
async fn test_aster_dav_fs_remove_dir_publishes_storage_event() {
    use aster_drive::services::auth::local;
    use aster_drive::services::storage_change_service::{
        StorageChangeKind, StorageChangeWorkspace,
    };
    use aster_drive::webdav::fs::AsterDavFs;
    use aster_drive::webdav::path_resolver::resolve_path;

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davfsremovedir",
        "davfsremovedir@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let (projects, docs, _reports, _file, _contents) =
        seed_nested_file(&state, user.id, None).await;

    let mut rx = state.storage_change_tx.subscribe();
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);

    let target = DavPath::new("/projects/docs/").unwrap();
    dav_fs.remove_dir(&target).await.unwrap();

    let event = recv_storage_event(&mut rx).await;
    assert_eq!(event.kind, StorageChangeKind::FolderTrashed);
    assert!(matches!(
        event.workspace,
        Some(StorageChangeWorkspace::Personal)
    ));
    assert!(event.file_ids.is_empty());
    assert_eq!(event.folder_ids, vec![docs.id]);
    assert_eq!(event.affected_parent_ids, vec![projects.id]);
    assert!(!event.root_affected);
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &target, None).await,
        Err(FsError::NotFound)
    ));
}

#[actix_web::test]
async fn test_aster_dav_fs_copy_folder_publishes_storage_event() {
    use aster_drive::services::auth::local;
    use aster_drive::services::storage_change_service::{
        StorageChangeKind, StorageChangeWorkspace,
    };
    use aster_drive::webdav::fs::AsterDavFs;
    use aster_drive::webdav::path_resolver::{ResolvedNode, resolve_path};

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davfscopyfolder",
        "davfscopyfolder@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let (projects, _docs, _reports, _file, _contents) =
        seed_nested_file(&state, user.id, None).await;

    let mut rx = state.storage_change_tx.subscribe();
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);

    let source = DavPath::new("/projects/docs/").unwrap();
    let destination = DavPath::new("/projects/docs-copy/").unwrap();
    dav_fs.copy(&source, &destination).await.unwrap();

    let event = recv_storage_event(&mut rx).await;
    assert_eq!(event.kind, StorageChangeKind::FolderCreated);
    assert!(matches!(
        event.workspace,
        Some(StorageChangeWorkspace::Personal)
    ));
    assert!(event.file_ids.is_empty());
    assert_eq!(event.folder_ids.len(), 1);
    assert_eq!(event.affected_parent_ids, vec![projects.id]);
    assert!(!event.root_affected);

    match resolve_path(
        state.writer_db(),
        user.id,
        &DavPath::new("/projects/docs-copy/reports/q1.txt").unwrap(),
        None,
    )
    .await
    .unwrap()
    {
        ResolvedNode::File(_) => {}
        other => panic!("expected copied nested file, got {other:?}"),
    }
}
