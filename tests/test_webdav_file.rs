//! 集成测试：`webdav_file`。

#[macro_use]
mod common;

use std::io::SeekFrom;

use aster_drive::runtime::SharedRuntimeState;
use aster_drive::webdav::dav::{DavFile, DavFileSystem, FsError, OpenOptions};
use bytes::Bytes;

fn write_temp_fixture(name: &str, contents: &str) -> String {
    let dir = format!("/tmp/asterdrive-webdav-file-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/{name}");
    std::fs::write(&path, contents).unwrap();
    path
}

fn snapshot_dir_tree(
    path: &std::path::Path,
) -> std::io::Result<std::collections::BTreeSet<String>> {
    fn walk(
        root: &std::path::Path,
        current: &std::path::Path,
        entries: &mut std::collections::BTreeSet<String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = std::collections::BTreeSet::new();
    if !path.exists() {
        return Ok(entries);
    }
    walk(path, path, &mut entries)?;
    Ok(entries)
}

#[actix_web::test]
async fn test_aster_dav_file_write_mode_persists_empty_and_written_content() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::auth::local;
    use aster_drive::webdav::file::AsterDavFile;

    let state = common::setup().await;
    let user = local::register(
        &state,
        "davfilewriter",
        "davfilewriter@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let mut empty_file = AsterDavFile::for_write(
        state.clone(),
        user.id,
        None,
        "empty-dav-file.txt".to_string(),
        None,
        None,
    )
    .await
    .unwrap();

    empty_file.metadata().await.unwrap();
    assert!(matches!(
        empty_file.read_bytes(1).await,
        Err(FsError::Forbidden)
    ));
    assert_eq!(empty_file.seek(SeekFrom::Start(0)).await.unwrap(), 0);
    empty_file.flush().await.unwrap();

    let empty_stored =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "empty-dav-file.txt")
            .await
            .unwrap()
            .expect("empty WebDAV flush should create a zero-byte file record");
    assert_eq!(empty_stored.size, 0);

    let mut written_file = AsterDavFile::for_write(
        state.clone(),
        user.id,
        None,
        "buffered-dav-file.txt".to_string(),
        None,
        None,
    )
    .await
    .unwrap();

    written_file
        .write_bytes(Bytes::from_static(b"hello "))
        .await
        .unwrap();
    assert_eq!(written_file.seek(SeekFrom::Current(0)).await.unwrap(), 6);
    written_file
        .write_buf(Box::new(Bytes::from_static(b"world")))
        .await
        .unwrap();
    written_file.flush().await.unwrap();

    let stored = file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        None,
        "buffered-dav-file.txt",
    )
    .await
    .unwrap()
    .expect("buffered WebDAV flush should create a file record");
    assert_eq!(stored.size, 11);
}

#[actix_web::test]
async fn test_aster_dav_fs_reports_quota_and_roundtrips_custom_props() {
    use aster_drive::db::repository::property_repo;
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::{auth::local, file_service};
    use aster_drive::types::EntityType;
    use aster_drive::webdav::dav::{DavFileSystem, DavPath, DavProp};
    use aster_drive::webdav::fs::AsterDavFs;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = local::register(&state, "davfsprops", "davfsprops@example.com", "pass1234")
        .await
        .unwrap();

    let content = "quota props";
    let temp_path = write_temp_fixture("quota-props.txt", content);
    file_service::store_from_temp(
        &state,
        user.id,
        file_service::StoreFromTempRequest::new(
            None,
            "quota-props.txt",
            &temp_path,
            content.len() as i64,
        ),
    )
    .await
    .unwrap();

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let file_path = DavPath::new("/quota-props.txt").unwrap();

    assert!(!dav_fs.have_props(&file_path).await);

    let (used, total) = dav_fs.get_quota().await.unwrap();
    assert_eq!(used, content.len() as u64);
    assert_eq!(total, None);

    let mut updated_user: aster_drive::entities::user::ActiveModel =
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .into();
    updated_user.storage_quota = Set(128);
    updated_user.update(state.writer_db()).await.unwrap();

    let (used, total) = dav_fs.get_quota().await.unwrap();
    assert_eq!(used, content.len() as u64);
    assert_eq!(total, Some(128));

    let set_results = dav_fs
        .patch_props(
            &file_path,
            vec![(
                true,
                DavProp {
                    name: "color".to_string(),
                    prefix: None,
                    namespace: Some("urn:aster:test".to_string()),
                    xml: Some(b"<A:color xmlns:A=\"urn:aster:test\">blue</A:color>".to_vec()),
                },
            )],
        )
        .await
        .unwrap();
    assert_eq!(set_results.len(), 1);
    assert_eq!(set_results[0].0, http::StatusCode::OK);
    assert!(dav_fs.have_props(&file_path).await);

    let stored_file = aster_drive::db::repository::file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        None,
        "quota-props.txt",
    )
    .await
    .unwrap()
    .expect("stored file should exist");
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        stored_file.id,
        "system.archive_preview",
        "zip_manifest.v2",
        Some("cached"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        stored_file.id,
        "DAV:",
        "displayname",
        Some("blocked"),
    )
    .await
    .unwrap();

    let props_without_content = dav_fs.get_props(&file_path, false).await.unwrap();
    assert_eq!(props_without_content.len(), 1);
    assert_eq!(
        props_without_content[0].namespace.as_deref(),
        Some("urn:aster:test")
    );
    assert!(props_without_content[0].xml.is_none());

    let props_with_content = dav_fs.get_props(&file_path, true).await.unwrap();
    assert_eq!(props_with_content.len(), 1);
    let xml = props_with_content[0]
        .xml
        .as_deref()
        .expect("stored property should include XML content");
    let color_prop = xmltree::Element::parse(std::io::Cursor::new(xml))
        .expect("stored property XML should parse");
    assert_eq!(color_prop.name, "color");
    assert_eq!(color_prop.namespace.as_deref(), Some("urn:aster:test"));
    assert_eq!(color_prop.get_text().as_deref(), Some("blue"));

    let remove_results = dav_fs
        .patch_props(
            &file_path,
            vec![(
                false,
                DavProp {
                    name: "color".to_string(),
                    prefix: None,
                    namespace: Some("urn:aster:test".to_string()),
                    xml: None,
                },
            )],
        )
        .await
        .unwrap();
    assert_eq!(remove_results.len(), 1);
    assert_eq!(remove_results[0].0, http::StatusCode::OK);
    assert!(!dav_fs.have_props(&file_path).await);

    let missing_path = DavPath::new("/missing.txt").unwrap();
    assert!(matches!(
        dav_fs.get_props(&missing_path, false).await,
        Err(FsError::NotFound)
    ));
}

#[actix_web::test]
async fn test_aster_dav_fs_open_read_is_rejected_without_temp_files() {
    use aster_drive::services::{auth::local, file_service};
    use aster_drive::webdav::dav::DavPath;
    use aster_drive::webdav::fs::AsterDavFs;

    let state = common::setup().await;
    let user = local::register(&state, "davreadfb", "davreadfb@example.com", "pass1234")
        .await
        .unwrap();

    let content = "buffered read fallback";
    let temp_path = write_temp_fixture("read-fallback.txt", content);
    file_service::store_from_temp(
        &state,
        user.id,
        file_service::StoreFromTempRequest::new(
            None,
            "read-fallback.txt",
            &temp_path,
            content.len() as i64,
        ),
    )
    .await
    .unwrap();

    let runtime_temp_dir =
        aster_drive::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let runtime_path = std::path::Path::new(&runtime_temp_dir);
    let snapshot_before = snapshot_dir_tree(runtime_path).unwrap();

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    assert!(matches!(
        dav_fs
            .open(
                &DavPath::new("/read-fallback.txt").unwrap(),
                OpenOptions::read(),
            )
            .await,
        Err(FsError::Forbidden)
    ));

    let snapshot_after = snapshot_dir_tree(runtime_path).unwrap();
    assert_eq!(
        snapshot_after, snapshot_before,
        "rejecting WebDAV open(read) should not create runtime temp files"
    );
}
