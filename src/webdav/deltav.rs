//! RFC3253 DeltaV 最小子集 — 版本历史查询
//!
//! 自研 WebDAV handler 在这里承接 REPORT / VERSION-CONTROL，
//! 利用已有的 file_versions 表返回最小 DeltaV 能力。

use actix_web::HttpResponse;
use actix_web::http::{StatusCode, Uri};
use sea_orm::DatabaseConnection;
use xmltree::Element;

use crate::db::repository::{file_repo, user_repo, version_repo};
use crate::webdav::auth::WebdavAuthResult;
use crate::webdav::dav::DavPath;
use crate::webdav::path_resolver::{self, ResolvedNode};
use crate::webdav::{
    XmlSafetyError, href_for_relative, responses, webdav_xml_root_local_name, xml_response,
};

/// 处理 REPORT 方法（cadaver `history` 发送 `DAV:version-tree`）
pub(crate) async fn handle_report(
    uri: &Uri,
    body_bytes: &[u8],
    db: &DatabaseConnection,
    auth: &WebdavAuthResult,
    prefix: &str,
) -> HttpResponse {
    // 解析 XML body，确认是 version-tree 报告
    let root_name = match webdav_xml_root_local_name(body_bytes) {
        Ok(root_name) => root_name,
        Err(XmlSafetyError::ExternalEntity) => return responses::no_external_entities(),
        Err(XmlSafetyError::TooDeep | XmlSafetyError::Malformed) => {
            return error_response(StatusCode::BAD_REQUEST, "Invalid XML body");
        }
    };

    if root_name != "version-tree" {
        return error_response(
            StatusCode::NOT_IMPLEMENTED,
            &format!("Unsupported REPORT type: {root_name}"),
        );
    }

    // 从 URI 中去掉 prefix 得到文件路径
    let path_str = uri.path();
    let relative = path_str.strip_prefix(prefix).unwrap_or(path_str);

    // 构造一个 DavPath 用于路径解析
    let dav_path = match DavPath::new(relative) {
        Ok(p) => p,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "Invalid path"),
    };

    let node =
        match path_resolver::resolve_path_in_scope(db, auth.scope, &dav_path, auth.root_folder_id)
            .await
        {
            Ok(n) => n,
            Err(_) => return error_response(StatusCode::NOT_FOUND, "Not Found"),
        };

    let file = match node {
        ResolvedNode::File(f) => f,
        _ => {
            return error_response(
                StatusCode::CONFLICT,
                "Version history is only available for files",
            );
        }
    };
    let decoded_relative = String::from_utf8_lossy(dav_path.as_bytes()).into_owned();

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

    // 构建 207 Multi-Status XML
    let mut multistatus = Element::new("D:multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    // 当前版本（活跃版本）
    if let Some(blob) = &current_blob {
        let href = href_for_relative(prefix, &decoded_relative);
        let response =
            build_version_response(&href, "current", blob.size, &file.updated_at, &creator);
        multistatus
            .children
            .push(xmltree::XMLNode::Element(response));
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
        let response = build_version_response(
            &href,
            &format!("V{}", ver.version),
            size,
            &ver.created_at,
            &creator,
        );
        multistatus
            .children
            .push(xmltree::XMLNode::Element(response));
    }

    xml_response(multistatus, StatusCode::MULTI_STATUS)
}

/// 处理 VERSION-CONTROL 方法（所有文件自动版本控制，直接返回 200）
pub(crate) async fn handle_version_control(
    uri: &Uri,
    db: &DatabaseConnection,
    auth: &WebdavAuthResult,
    prefix: &str,
) -> HttpResponse {
    let path_str = uri.path();
    let relative = path_str.strip_prefix(prefix).unwrap_or(path_str);

    let dav_path = match DavPath::new(relative) {
        Ok(p) => p,
        Err(_) => return error_response(StatusCode::BAD_REQUEST, "Invalid path"),
    };

    match path_resolver::resolve_path_in_scope(db, auth.scope, &dav_path, auth.root_folder_id).await
    {
        Ok(ResolvedNode::File(_)) => {
            responses::text(StatusCode::OK, "Already under version control")
        }
        Ok(_) => error_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "Only files support version control",
        ),
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
) -> Element {
    let mut response = Element::new("D:response");

    // <D:href>
    let mut href_el = Element::new("D:href");
    href_el
        .children
        .push(xmltree::XMLNode::Text(href.to_string()));
    response.children.push(xmltree::XMLNode::Element(href_el));

    // <D:propstat>
    let mut propstat = Element::new("D:propstat");

    let mut prop = Element::new("D:prop");

    // <D:version-name>
    let mut vname = Element::new("D:version-name");
    vname
        .children
        .push(xmltree::XMLNode::Text(version_name.to_string()));
    prop.children.push(xmltree::XMLNode::Element(vname));

    // <D:creator-displayname>
    let mut cname = Element::new("D:creator-displayname");
    cname
        .children
        .push(xmltree::XMLNode::Text(creator.to_string()));
    prop.children.push(xmltree::XMLNode::Element(cname));

    // <D:getcontentlength>
    let mut clen = Element::new("D:getcontentlength");
    clen.children.push(xmltree::XMLNode::Text(size.to_string()));
    prop.children.push(xmltree::XMLNode::Element(clen));

    // <D:getlastmodified>
    let mut lmod = Element::new("D:getlastmodified");
    let rfc2822 = modified.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    lmod.children.push(xmltree::XMLNode::Text(rfc2822));
    prop.children.push(xmltree::XMLNode::Element(lmod));

    propstat.children.push(xmltree::XMLNode::Element(prop));

    // <D:status>
    let mut status = Element::new("D:status");
    status
        .children
        .push(xmltree::XMLNode::Text("HTTP/1.1 200 OK".to_string()));
    propstat.children.push(xmltree::XMLNode::Element(status));

    response.children.push(xmltree::XMLNode::Element(propstat));

    response
}

fn error_response(status: StatusCode, msg: &str) -> HttpResponse {
    responses::text(status, msg)
}
