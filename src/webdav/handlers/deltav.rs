//! RFC3253 DeltaV 最小子集 — 版本历史查询
//!
//! 自研 WebDAV handler 在这里承接 REPORT / VERSION-CONTROL，
//! 利用已有的 file_versions 表返回最小 DeltaV 能力。

use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use aster_forge_utils::http_validators::format_http_date;
use aster_forge_webdav::{
    DavRequestHead, DavResourceKind, DavVersionXml, validate_version_tree_report,
    version_control_response, version_tree_non_file_response, version_tree_report_error_response,
    version_tree_response,
};
use sea_orm::DatabaseConnection;

use crate::db::repository::{file_repo, user_repo, version_repo};
use crate::webdav::auth::WebdavAuthResult;
use crate::webdav::backend::path_resolver::{self, ResolvedNode};
use crate::webdav::{href_for_relative, responses};

/// 处理 REPORT 方法（cadaver `history` 发送 `DAV:version-tree`）
pub(crate) async fn handle_report(
    request_head: &DavRequestHead,
    body_bytes: &[u8],
    db: &DatabaseConnection,
    auth: &WebdavAuthResult,
    prefix: &str,
) -> HttpResponse {
    if let Err(error) = validate_version_tree_report(body_bytes) {
        return match version_tree_report_error_response(&error) {
            Ok(response) => aster_forge_webdav::actix::into_response(response),
            Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
        };
    }

    let dav_path = &request_head.target;

    let node =
        match path_resolver::resolve_path_in_scope(db, auth.scope, dav_path, auth.root_folder_id)
            .await
        {
            Ok(n) => n,
            Err(_) => return error_response(StatusCode::NOT_FOUND, "Not Found"),
        };

    let file = match node {
        ResolvedNode::File(f) => f,
        _ => return aster_forge_webdav::actix::into_response(version_tree_non_file_response()),
    };
    let decoded_relative = dav_path.as_str().to_owned();

    // 查版本列表
    let versions = match version_repo::find_by_file_id(db, file.id).await {
        Ok(v) => v,
        Err(_) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to query versions",
            );
        }
    };

    // 查用户名
    let creator = match file.created_by_user_id {
        Some(user_id) => user_repo::find_by_id(db, user_id)
            .await
            .map(|u| u.username)
            .unwrap_or_else(|_| file.created_by_username.clone()),
        None => file.created_by_username.clone(),
    };
    let creator = if creator.is_empty() {
        "unknown".to_string()
    } else {
        creator
    };

    // 查当前版本的 blob 信息
    let current_blob = file_repo::find_blob_by_id(db, file.blob_id).await.ok();

    let mut version_responses = Vec::with_capacity(versions.len() + 1);

    // 当前版本（活跃版本）
    if let Some(blob) = &current_blob {
        let href = href_for_relative(prefix, &decoded_relative);
        version_responses.push(build_version_response(
            &href,
            "current",
            blob.size,
            &file.updated_at,
            &creator,
        ));
    }

    // 历史版本
    // 批量查 blob 信息
    let blob_ids: Vec<i64> = versions.iter().map(|v| v.blob_id).collect();
    let blobs = file_repo::find_blobs_by_ids(db, &blob_ids)
        .await
        .unwrap_or_default();

    for ver in &versions {
        let size = blobs.get(&ver.blob_id).map(|b| b.size).unwrap_or(ver.size);

        let href = format!(
            "{}?v={}",
            href_for_relative(prefix, &decoded_relative),
            ver.version
        );
        version_responses.push(build_version_response(
            &href,
            &format!("V{}", ver.version),
            size,
            &ver.created_at,
            &creator,
        ));
    }

    match version_tree_response(version_responses) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// 处理 VERSION-CONTROL 方法（所有文件自动版本控制，直接返回 200）
pub(crate) async fn handle_version_control(
    request_head: &DavRequestHead,
    db: &DatabaseConnection,
    auth: &WebdavAuthResult,
) -> HttpResponse {
    match path_resolver::resolve_path_in_scope(
        db,
        auth.scope,
        &request_head.target,
        auth.root_folder_id,
    )
    .await
    {
        Ok(ResolvedNode::File(_)) => aster_forge_webdav::actix::into_response(
            version_control_response(DavResourceKind::File),
        ),
        Ok(_) => aster_forge_webdav::actix::into_response(version_control_response(
            DavResourceKind::Collection,
        )),
        Err(_) => error_response(StatusCode::NOT_FOUND, "Not Found"),
    }
}

/// 构建单个版本的 `<D:response>` 元素
fn build_version_response(
    href: &str,
    version_name: &str,
    size: i64,
    modified: &chrono::DateTime<chrono::Utc>,
    creator: &str,
) -> DavVersionXml {
    DavVersionXml {
        href: href.to_owned(),
        version_name: version_name.to_owned(),
        creator: creator.to_owned(),
        content_length: size,
        last_modified: format_http_date((*modified).into()),
    }
}

fn error_response(status: StatusCode, msg: &str) -> HttpResponse {
    responses::text(status, msg)
}
