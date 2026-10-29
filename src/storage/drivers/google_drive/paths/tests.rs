use super::*;
use crate::storage::test_support::{HttpFixture, HttpReply};
use serde_json::json;

#[derive(Debug)]
struct Token;
#[async_trait]
impl GoogleDriveTokenProvider for Token {
    async fn access_token(&self) -> Result<String> {
        Ok("private-token".into())
    }
}
fn folder(id: &str) -> serde_json::Value {
    json!({"id":id,"mimeType":FOLDER_MIME,"name":"folder"})
}
fn list(files: Vec<serde_json::Value>) -> HttpReply {
    HttpReply::json(200, json!({"files":files}))
}
fn driver(peer: &HttpFixture, appdata: bool, shared: bool, base: &str) -> GoogleDriveDriver {
    let mut driver = GoogleDriveDriver::new(
        Arc::new(Token),
        if appdata { None } else { Some("pool".into()) },
        shared.then(|| "shared".into()),
        appdata,
        base.into(),
        Duration::from_secs(10),
    )
    .unwrap();
    driver.api = format!("{}/drive/v3", peer.base);
    driver.upload = format!("{}/upload/drive/v3/files", peer.base);
    driver
}

#[test]
fn normalization_rejects_escape_and_budgets_while_retaining_unicode_names() {
    assert_eq!(
        normalize_base_path(" /AsterDrive//图片/ ").unwrap(),
        "AsterDrive/图片"
    );
    assert_eq!(normalize_base_path("///").unwrap(), "");
    for path in [
        ".",
        "a/../b",
        "a/./b",
        "a\\b",
        "x\0y",
        "x\ny",
        "a/ /b",
        "https://drive.test/path",
    ] {
        assert!(normalize_base_path(path).is_err(), "accepted {path:?}");
    }
    assert!(normalize_base_path(&"x".repeat(256)).is_err());
    assert!(normalize_base_path(&["folder"; 33].join("/")).is_err());
    assert!(normalize_base_path(&["folder12345678901234567890123456789"; 32].join("/")).is_err());
}

#[tokio::test]
async fn existing_base_folders_scope_object_lookup_to_the_final_parent_in_all_spaces() {
    for (appdata, shared) in [(false, false), (false, true), (true, false)] {
        let peer = HttpFixture::new([
            list(vec![folder("base")]),
            list(vec![folder("nested")]),
            list(vec![json!({"id":"blob","size":"6"})]),
        ])
        .await;
        assert!(
            driver(&peer, appdata, shared, "AsterDrive/blobs")
                .exists("files/key")
                .await
                .unwrap()
        );
        let req = peer.requests();
        assert!(req[0].query["q"].contains(if appdata {
            "'appDataFolder' in parents"
        } else {
            "'pool' in parents"
        }));
        assert!(req[1].query["q"].contains("'base' in parents"));
        assert!(req[2].query["q"].contains("'nested' in parents"));
        assert!(req[2].query["q"].contains("asterdrive_storage_path"));
        assert!(req.iter().all(|r| r.method == "GET"
            && r.query["spaces"] == if appdata { "appDataFolder" } else { "drive" }));
        if shared {
            assert!(
                req.iter()
                    .all(|r| r.query["driveId"] == "shared" && r.query["corpora"] == "drive")
            );
        }
        peer.stop().await;
    }
}

#[tokio::test]
async fn missing_base_is_not_created_by_reads_metadata_delete_or_failed_copy_source() {
    let peer = HttpFixture::new((0..5).map(|_| list(vec![]))).await;
    let driver = driver(&peer, false, false, "missing");
    assert!(!driver.exists("files/key").await.unwrap());
    driver.delete("files/key").await.unwrap();
    assert_eq!(
        driver.metadata("files/key").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        driver.get("files/key").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        driver
            .copy_object("files/key", "files/copy")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    assert!(peer.requests().iter().all(|r| r.method == "GET"));
    peer.stop().await;
}

#[tokio::test]
async fn write_creates_and_confirms_missing_folders_then_uploads_beneath_them() {
    for appdata in [false, true] {
        let peer = HttpFixture::new([
            list(vec![]),
            HttpReply::json(200, folder("base")),
            list(vec![folder("base")]),
            list(vec![]),
            HttpReply::json(200, folder("nested")),
            list(vec![folder("nested")]),
            list(vec![]),
            HttpReply::text(200, "")
                .header("location", "{base}/upload/drive/v3/files?upload_id=session"),
            HttpReply::json(200, json!({"id":"object"})),
        ])
        .await;
        driver(&peer, appdata, false, "AsterDrive/blobs")
            .put("files/key", b"abc")
            .await
            .unwrap();
        let req = peer.requests();
        let first: serde_json::Value = serde_json::from_slice(&req[1].body).unwrap();
        assert_eq!(first["name"], "AsterDrive");
        assert_eq!(first["mimeType"], FOLDER_MIME);
        assert_eq!(
            first["parents"][0],
            if appdata { "appDataFolder" } else { "pool" }
        );
        let second: serde_json::Value = serde_json::from_slice(&req[4].body).unwrap();
        assert_eq!(second["parents"][0], "base");
        let object: serde_json::Value = serde_json::from_slice(&req[7].body).unwrap();
        assert_eq!(object["parents"][0], "nested");
        assert_eq!(object["appProperties"][APP_KEY], "files/key");
        assert_eq!(req[8].body, b"abc");
        peer.stop().await;
    }
}

#[tokio::test]
async fn file_shortcut_duplicate_or_incomplete_components_cannot_authorize_writes() {
    for (reply, kind) in [
        (
            list(vec![
                json!({"id":"file","mimeType":"application/octet-stream"}),
            ]),
            StorageErrorKind::Misconfigured,
        ),
        (
            list(vec![
                json!({"id":"shortcut","mimeType":"application/vnd.google-apps.shortcut"}),
            ]),
            StorageErrorKind::Misconfigured,
        ),
        (
            list(vec![folder("one"), folder("two")]),
            StorageErrorKind::Misconfigured,
        ),
        (
            HttpReply::json(200, json!({"files":[],"incompleteSearch":true})),
            StorageErrorKind::Transient,
        ),
        (
            HttpReply::text(403, "private-token"),
            StorageErrorKind::Permission,
        ),
    ] {
        let peer = HttpFixture::new([reply]).await;
        let error = driver(&peer, false, false, "base")
            .put("files/key", b"x")
            .await
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(!error.to_string().contains("private-token"));
        assert!(peer.requests().iter().all(|r| r.method == "GET"));
        peer.stop().await;
    }
}

#[tokio::test]
async fn concurrent_creation_ambiguity_and_missing_confirmation_stop_before_object_upload() {
    for (last, kind) in [
        (
            list(vec![folder("ours"), folder("other")]),
            StorageErrorKind::Misconfigured,
        ),
        (list(vec![]), StorageErrorKind::Transient),
        (list(vec![folder("other")]), StorageErrorKind::Transient),
    ] {
        let peer =
            HttpFixture::new([list(vec![]), HttpReply::json(200, folder("ours")), last]).await;
        assert_eq!(
            driver(&peer, false, false, "base")
                .put("files/key", b"x")
                .await
                .unwrap_err()
                .kind(),
            kind
        );
        assert_eq!(peer.requests().len(), 3);
        assert!(
            peer.requests()
                .iter()
                .all(|r| !r.path.starts_with("/upload/"))
        );
        peer.stop().await;
    }
}

#[tokio::test]
async fn quoted_names_are_escaped_once_in_queries_and_preserved_in_creation_json() {
    let peer = HttpFixture::new([
        list(vec![]),
        HttpReply::json(200, folder("base")),
        list(vec![folder("base")]),
    ])
    .await;
    assert_eq!(
        driver(&peer, false, false, "猫猫's files")
            .resolve_base_folder(true)
            .await
            .unwrap()
            .as_deref(),
        Some("base")
    );
    let req = peer.requests();
    assert!(req[0].query["q"].contains("name='猫猫\\'s files'"));
    let body: serde_json::Value = serde_json::from_slice(&req[1].body).unwrap();
    assert_eq!(body["name"], "猫猫's files");
    peer.stop().await;
}

#[tokio::test]
async fn duplicate_folder_names_across_short_pages_fail_closed() {
    let peer = HttpFixture::new([
        HttpReply::json(
            200,
            json!({"files":[folder("one")],"nextPageToken":"second"}),
        ),
        list(vec![folder("two")]),
    ])
    .await;
    assert_eq!(
        driver(&peer, false, false, "base")
            .resolve_base_folder(false)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Misconfigured
    );
    assert_eq!(peer.requests()[1].query["pageToken"], "second");
    peer.stop().await;
}

#[tokio::test]
async fn deletion_is_a_real_idempotent_delete_under_the_selected_base() {
    let peer = HttpFixture::new([
        list(vec![folder("base")]),
        list(vec![json!({"id":"object"})]),
        HttpReply::text(204, ""),
    ])
    .await;
    driver(&peer, false, true, "base")
        .delete("files/key")
        .await
        .unwrap();
    let req = peer.requests();
    assert_eq!(req[2].method, "DELETE");
    assert_eq!(req[2].path, "/drive/v3/files/object");
    assert_eq!(req[2].query["supportsAllDrives"], "true");
    peer.stop().await;
}
