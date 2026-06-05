//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use actix_web::http::{StatusCode, header};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::{StreamExt, pin_mut};
use xmltree::XMLNode;

use crate::webdav::dav::{DavFileSystem, DavLockSystem, DavPath, FsError, ReadDirMeta};
use crate::webdav::protocol::{self, Depth};
use crate::webdav::{
    child_relative_path, dav_element, decoded_path_string, ensure_parent_unlocked,
    ensure_system_file_name_allowed, ensure_unlocked, fs, fs_error_response, href_for_dav_path,
    href_for_relative, lock_token_submitted_element, multi_status, parent_relative_path,
    request_origin, request_path, status_element, system_file, text_element, xml_response,
};

#[derive(Clone)]
struct MultiStatusFailure {
    path: DavPath,
    status: StatusCode,
    lock_path: Option<DavPath>,
}

struct DavChild {
    path: DavPath,
    relative: String,
    is_dir: bool,
}

struct PartialMutationOutcome {
    failures: Vec<MultiStatusFailure>,
    destination_exists: bool,
}

struct PartialMutationContext<'a> {
    dav_fs: &'a fs::AsterDavFs,
    lock_system: &'a dyn DavLockSystem,
    req: &'a HttpRequest,
    prefix: &'a str,
    is_move: bool,
}

struct PartialMutationNode<'a> {
    source: &'a DavPath,
    source_relative: &'a str,
    destination: &'a DavPath,
    destination_relative: &'a str,
}

pub(crate) async fn handle_mkcol(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    payload: &mut web::Payload,
) -> HttpResponse {
    if let Err(resp) = ensure_empty_body(payload).await {
        return resp;
    }

    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    if relative == "/" {
        return HttpResponse::MethodNotAllowed().finish();
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }

    if let Err(resp) = ensure_parent_exists(dav_fs, &relative).await {
        return resp;
    }
    let (request_scheme, request_host) = request_origin(req);
    if let Err(resp) = protocol::ensure_if_header(
        req.headers(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = ensure_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = ensure_parent_unlocked(
        lock_system,
        &relative,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }

    match dav_fs.create_dir(&path).await {
        Ok(()) => HttpResponse::Created()
            .insert_header((
                header::CONTENT_LOCATION,
                href_for_relative(prefix, &relative),
            ))
            .finish(),
        Err(FsError::Exists) => HttpResponse::MethodNotAllowed().finish(),
        Err(FsError::NotFound) => HttpResponse::Conflict().finish(),
        Err(err) => fs_error_response(err),
    }
}

pub(crate) async fn handle_delete(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
) -> HttpResponse {
    let depth = match protocol::parse_delete_depth(req.headers()) {
        Ok(depth) => depth,
        Err(resp) => return resp,
    };

    let (path, relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if meta.is_dir() && !depth.is_infinity() {
        return HttpResponse::BadRequest().finish();
    }
    if let Err(resp) = protocol::evaluate_http_etag_preconditions(
        req.headers(),
        true,
        meta.etag().as_deref(),
        false,
    ) {
        return resp;
    }
    let (request_scheme, request_host) = request_origin(req);
    if let Err(resp) = protocol::ensure_if_header(
        req.headers(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if meta.is_dir() {
        if let Some(resp) =
            locked_multi_status_response(lock_system, &path, true, prefix, req).await
        {
            return resp;
        }
    } else if let Err(resp) = ensure_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = ensure_parent_unlocked(
        lock_system,
        &relative,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }

    let result = if meta.is_dir() {
        dav_fs.remove_dir(&path).await
    } else {
        dav_fs.remove_file(&path).await
    };
    match result {
        Ok(()) => {
            if lock_system.delete(&path).await.is_err() {
                tracing::warn!(
                    path = %decoded_path_string(&path),
                    "failed to delete WebDAV locks after resource deletion"
                );
            }
            HttpResponse::NoContent().finish()
        }
        Err(err) => fs_error_response(err),
    }
}

pub(crate) async fn handle_copy_move(
    req: &HttpRequest,
    dav_fs: &fs::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    is_move: bool,
) -> HttpResponse {
    let depth = if is_move {
        match protocol::parse_move_depth(req.headers()) {
            Ok(depth) => depth,
            Err(resp) => return resp,
        }
    } else {
        match protocol::parse_copy_depth(req.headers()) {
            Ok(depth) => depth,
            Err(resp) => return resp,
        }
    };

    let (source, source_relative) = match request_path(req, prefix) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let (request_scheme, request_host) = request_origin(req);
    let destination_relative = match protocol::destination_relative_path(
        req.headers(),
        prefix,
        &request_scheme,
        &request_host,
    ) {
        Ok(path) => path,
        Err(resp) => return resp,
    };
    if same_resource_path(&source_relative, &destination_relative) {
        return HttpResponse::Forbidden().finish();
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &destination_relative) {
        return resp;
    }
    if let Err(resp) = ensure_parent_exists(dav_fs, &destination_relative).await {
        return resp;
    }

    let destination = match DavPath::new(&destination_relative) {
        Ok(path) => path,
        Err(_) => return HttpResponse::BadRequest().body("Invalid destination path"),
    };

    let source_meta = match dav_fs.metadata(&source).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if let Err(resp) = protocol::evaluate_http_etag_preconditions(
        req.headers(),
        true,
        source_meta.etag().as_deref(),
        false,
    ) {
        return resp;
    }
    if source_meta.is_dir() {
        if is_move && !depth.is_infinity() {
            return HttpResponse::BadRequest().finish();
        }
        if !is_move && depth == Depth::One {
            return HttpResponse::BadRequest().finish();
        }
    }
    let recursive_collection_copy_or_move =
        source_meta.is_dir() && (is_move || depth != Depth::Zero);
    if recursive_collection_copy_or_move
        && is_descendant_path(&source_relative, &destination_relative)
    {
        return HttpResponse::Forbidden().finish();
    }
    if let Err(resp) = protocol::ensure_if_header(
        req.headers(),
        dav_fs,
        lock_system,
        &source,
        prefix,
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }
    if is_move
        && let Err(resp) = ensure_unlocked(
            lock_system,
            &source,
            false,
            prefix,
            req.headers(),
            &request_scheme,
            &request_host,
        )
        .await
    {
        return resp;
    }
    if is_move
        && let Err(resp) = ensure_parent_unlocked(
            lock_system,
            &source_relative,
            prefix,
            req.headers(),
            &request_scheme,
            &request_host,
        )
        .await
    {
        return resp;
    }

    let destination_meta = match dav_fs.metadata(&destination).await {
        Ok(meta) => Some(meta),
        Err(FsError::NotFound) => None,
        Err(err) => return fs_error_response(err),
    };
    let destination_exists = destination_meta.is_some();
    let overwrite = match protocol::parse_overwrite(req.headers()) {
        Ok(overwrite) => overwrite,
        Err(resp) => return resp,
    };
    if !overwrite && destination_exists {
        return HttpResponse::PreconditionFailed().finish();
    }
    let destination_is_collection = destination_meta.as_ref().is_some_and(|meta| meta.is_dir());
    let destination_deep =
        destination_is_collection || source_meta.is_dir() && (is_move || depth != Depth::Zero);
    if !destination_deep
        && let Err(resp) = ensure_unlocked(
            lock_system,
            &destination,
            false,
            prefix,
            req.headers(),
            &request_scheme,
            &request_host,
        )
        .await
    {
        return resp;
    }
    if let Err(resp) = ensure_parent_unlocked(
        lock_system,
        &destination_relative,
        prefix,
        req.headers(),
        &request_scheme,
        &request_host,
    )
    .await
    {
        return resp;
    }

    if recursive_collection_copy_or_move {
        let source_conflicts = if is_move {
            unsubmitted_lock_conflicts(lock_system, &source, true, prefix, req).await
        } else {
            Vec::new()
        };
        let destination_conflicts =
            unsubmitted_lock_conflicts(lock_system, &destination, true, prefix, req).await;
        if !source_conflicts.is_empty() || !destination_conflicts.is_empty() {
            let ctx = PartialMutationContext {
                dav_fs,
                lock_system,
                req,
                prefix,
                is_move,
            };
            let root = PartialMutationNode {
                source: &source,
                source_relative: &source_relative,
                destination: &destination,
                destination_relative: &destination_relative,
            };
            let outcome = match partial_recursive_copy_move(
                &ctx,
                root,
                destination_exists,
                destination_is_collection,
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(err) => return fs_error_response(err),
            };
            if !outcome.failures.is_empty() {
                return multi_status_failure_response(prefix, &outcome.failures);
            }
            if outcome.destination_exists {
                return no_store_response(StatusCode::NO_CONTENT);
            }
            return no_store_response(StatusCode::CREATED);
        }
    }

    if destination_deep
        && let Some(resp) =
            locked_multi_status_response(lock_system, &destination, true, prefix, req).await
    {
        return resp;
    }

    let result = if is_move {
        dav_fs.rename(&source, &destination).await
    } else if source_meta.is_dir() && depth == Depth::Zero {
        dav_fs.copy_dir_shallow(&source, &destination).await
    } else {
        dav_fs.copy(&source, &destination).await
    };

    match result {
        Ok(()) => {
            if is_move && lock_system.delete(&source).await.is_err() {
                tracing::warn!(
                    path = %source_relative,
                    "failed to delete WebDAV locks after move"
                );
            }
            if destination_exists {
                no_store_response(StatusCode::NO_CONTENT)
            } else {
                no_store_response(StatusCode::CREATED)
            }
        }
        Err(err) => fs_error_response(err),
    }
}

fn no_store_response(status: StatusCode) -> HttpResponse {
    HttpResponse::build(status)
        .insert_header((header::CACHE_CONTROL, "no-store"))
        .finish()
}

async fn locked_multi_status_response(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    req: &HttpRequest,
) -> Option<HttpResponse> {
    let mut conflicts = lock_system.conflicting_locks(path, deep).await;
    let (request_scheme, request_host) = request_origin(req);
    conflicts.retain(|lock| {
        let href = href_for_dav_path(prefix, &lock.path);
        let tokens = protocol::submitted_lock_tokens_for_path(
            req.headers(),
            &href,
            &request_scheme,
            &request_host,
        );
        !tokens.iter().any(|token| token == &lock.token)
    });
    if conflicts.is_empty() {
        return None;
    }

    Some(multi_status_locked_response(prefix, &conflicts))
}

fn multi_status_locked_response(
    prefix: &str,
    locks: &[crate::webdav::dav::DavLock],
) -> HttpResponse {
    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    for lock in locks {
        let mut response = dav_element("response");
        response.children.push(XMLNode::Element(text_element(
            "D:href",
            &href_for_dav_path(prefix, &lock.path),
        )));
        response
            .children
            .push(XMLNode::Element(status_element(StatusCode::LOCKED)));
        let mut error = dav_element("error");
        error
            .children
            .push(XMLNode::Element(lock_token_submitted_element(
                prefix, &lock.path,
            )));
        response.children.push(XMLNode::Element(error));
        multistatus.children.push(XMLNode::Element(response));
    }

    let mut response = xml_response(multistatus, multi_status());
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

async fn unsubmitted_lock_conflicts(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    req: &HttpRequest,
) -> Vec<crate::webdav::dav::DavLock> {
    let mut conflicts = lock_system.conflicting_locks(path, deep).await;
    let (request_scheme, request_host) = request_origin(req);
    conflicts.retain(|lock| {
        let href = href_for_dav_path(prefix, &lock.path);
        let tokens = protocol::submitted_lock_tokens_for_path(
            req.headers(),
            &href,
            &request_scheme,
            &request_host,
        );
        !tokens.iter().any(|token| token == &lock.token)
    });
    conflicts
}

async fn partial_recursive_copy_move(
    ctx: &PartialMutationContext<'_>,
    root: PartialMutationNode<'_>,
    destination_exists: bool,
    destination_is_collection: bool,
) -> Result<PartialMutationOutcome, FsError> {
    let mut failures = Vec::new();
    if destination_exists && !destination_is_collection {
        ctx.dav_fs.remove_file(root.destination).await?;
    } else if destination_exists && destination_is_collection {
        let conflicts = collect_lock_failures(ctx, root.destination, true).await;
        if !conflicts.is_empty() {
            failures.extend(conflicts);
        }
    }

    if !destination_exists {
        ctx.dav_fs
            .copy_dir_shallow(root.source, root.destination)
            .await?;
    }

    let source_children = collect_children(ctx.dav_fs, root.source, root.source_relative).await?;
    for child in source_children {
        let dest_relative = replace_relative_prefix(
            &child.relative,
            root.source_relative,
            root.destination_relative,
        );
        let dest_path = DavPath::new(&dest_relative).map_err(|_| FsError::BadRequest)?;
        let child_node = PartialMutationNode {
            source: &child.path,
            source_relative: &child.relative,
            destination: &dest_path,
            destination_relative: &dest_relative,
        };
        if child.is_dir {
            partial_recursive_copy_move_dir(ctx, child_node, &mut failures).await?;
        } else {
            partial_copy_move_file(ctx, child_node, &mut failures).await?;
        }
    }

    if ctx.is_move {
        let remaining = collect_children(ctx.dav_fs, root.source, root.source_relative).await?;
        if remaining.is_empty() {
            ctx.dav_fs.remove_dir(root.source).await?;
            if ctx.lock_system.delete(root.source).await.is_err() {
                tracing::warn!(path = %root.source_relative, "failed to delete WebDAV locks after partial move");
            }
        }
    }

    Ok(PartialMutationOutcome {
        failures,
        destination_exists,
    })
}

fn partial_recursive_copy_move_dir<'a>(
    ctx: &'a PartialMutationContext<'a>,
    node: PartialMutationNode<'a>,
    failures: &'a mut Vec<MultiStatusFailure>,
) -> futures::future::LocalBoxFuture<'a, Result<(), FsError>> {
    Box::pin(async move {
        if ctx.is_move {
            let conflicts = collect_lock_failures(ctx, node.source, true).await;
            if !conflicts.is_empty() {
                failures.extend(conflicts);
                return Ok(());
            }
        }

        let dest_meta = match ctx.dav_fs.metadata(node.destination).await {
            Ok(meta) => Some(meta),
            Err(FsError::NotFound) => None,
            Err(err) => return Err(err),
        };
        if dest_meta.as_ref().is_some_and(|meta| !meta.is_dir()) {
            let conflicts = collect_lock_failures(ctx, node.destination, false).await;
            if !conflicts.is_empty() {
                failures.extend(conflicts);
                return Ok(());
            }
            ctx.dav_fs.remove_file(node.destination).await?;
        } else if dest_meta.as_ref().is_some_and(|meta| meta.is_dir()) {
            let conflicts = collect_lock_failures(ctx, node.destination, true).await;
            if !conflicts.is_empty() {
                failures.extend(conflicts);
                return Ok(());
            }
        } else {
            ctx.dav_fs
                .copy_dir_shallow(node.source, node.destination)
                .await?;
        }

        let children = collect_children(ctx.dav_fs, node.source, node.source_relative).await?;
        for child in children {
            let dest_relative = replace_relative_prefix(
                &child.relative,
                node.source_relative,
                node.destination_relative,
            );
            let dest_path = DavPath::new(&dest_relative).map_err(|_| FsError::BadRequest)?;
            let child_node = PartialMutationNode {
                source: &child.path,
                source_relative: &child.relative,
                destination: &dest_path,
                destination_relative: &dest_relative,
            };
            if child.is_dir {
                partial_recursive_copy_move_dir(ctx, child_node, failures).await?;
            } else {
                partial_copy_move_file(ctx, child_node, failures).await?;
            }
        }

        if ctx.is_move {
            let remaining = collect_children(ctx.dav_fs, node.source, node.source_relative).await?;
            if remaining.is_empty() {
                ctx.dav_fs.remove_dir(node.source).await?;
                if ctx.lock_system.delete(node.source).await.is_err() {
                    tracing::warn!(path = %node.source_relative, "failed to delete WebDAV locks after partial move");
                }
            }
        }

        Ok(())
    })
}

async fn partial_copy_move_file(
    ctx: &PartialMutationContext<'_>,
    node: PartialMutationNode<'_>,
    failures: &mut Vec<MultiStatusFailure>,
) -> Result<(), FsError> {
    if ctx.is_move {
        let conflicts = collect_lock_failures(ctx, node.source, false).await;
        if !conflicts.is_empty() {
            failures.extend(conflicts);
            return Ok(());
        }
    }
    let dest_conflicts = collect_lock_failures(ctx, node.destination, false).await;
    if !dest_conflicts.is_empty() {
        failures.extend(dest_conflicts);
        return Ok(());
    }
    if ctx.is_move {
        ctx.dav_fs.rename(node.source, node.destination).await?;
        if ctx.lock_system.delete(node.source).await.is_err() {
            tracing::warn!(path = %decoded_path_string(node.source), "failed to delete WebDAV locks after partial file move");
        }
    } else {
        ctx.dav_fs.copy(node.source, node.destination).await?;
    }
    Ok(())
}

async fn collect_lock_failures(
    ctx: &PartialMutationContext<'_>,
    path: &DavPath,
    deep: bool,
) -> Vec<MultiStatusFailure> {
    unsubmitted_lock_conflicts(ctx.lock_system, path, deep, ctx.prefix, ctx.req)
        .await
        .into_iter()
        .map(|lock| MultiStatusFailure {
            path: (*lock.path).clone(),
            status: StatusCode::LOCKED,
            lock_path: Some((*lock.path).clone()),
        })
        .collect()
}

async fn collect_children(
    dav_fs: &fs::AsterDavFs,
    path: &DavPath,
    relative: &str,
) -> Result<Vec<DavChild>, FsError> {
    let entries = dav_fs.read_dir(path, ReadDirMeta::Data).await?;
    pin_mut!(entries);
    let mut children = Vec::new();
    while let Some(entry) = entries.next().await {
        let entry = entry?;
        let meta = entry.metadata().await?;
        let child_relative = child_relative_path(relative, &entry.name(), meta.is_dir());
        let child_path = DavPath::new(&child_relative).map_err(|_| FsError::GeneralFailure)?;
        children.push(DavChild {
            path: child_path,
            relative: child_relative,
            is_dir: meta.is_dir(),
        });
    }
    Ok(children)
}

fn replace_relative_prefix(path: &str, source_prefix: &str, destination_prefix: &str) -> String {
    let source_prefix = source_prefix.trim_end_matches('/');
    let destination_prefix = destination_prefix.trim_end_matches('/');
    let suffix = path.trim_start_matches(source_prefix);
    if suffix.is_empty() {
        format!("{destination_prefix}/")
    } else {
        format!("{destination_prefix}{suffix}")
    }
}

fn multi_status_failure_response(prefix: &str, failures: &[MultiStatusFailure]) -> HttpResponse {
    let mut multistatus = dav_element("multistatus");
    multistatus
        .attributes
        .insert("xmlns:D".to_string(), "DAV:".to_string());

    for failure in failures {
        let mut response = dav_element("response");
        response.children.push(XMLNode::Element(text_element(
            "D:href",
            &href_for_dav_path(prefix, &failure.path),
        )));
        response
            .children
            .push(XMLNode::Element(status_element(failure.status)));
        if failure.status == StatusCode::LOCKED {
            let lock_path = failure.lock_path.as_ref().unwrap_or(&failure.path);
            let mut error = dav_element("error");
            error
                .children
                .push(XMLNode::Element(lock_token_submitted_element(
                    prefix, lock_path,
                )));
            response.children.push(XMLNode::Element(error));
        }
        multistatus.children.push(XMLNode::Element(response));
    }

    let mut response = xml_response(multistatus, multi_status());
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

pub(crate) async fn ensure_empty_body(payload: &mut web::Payload) -> Result<(), HttpResponse> {
    while let Some(chunk) = payload.next().await {
        let chunk =
            chunk.map_err(|_| HttpResponse::BadRequest().body("Failed to read request body"))?;
        if !chunk.is_empty() {
            return Err(HttpResponse::UnsupportedMediaType().finish());
        }
    }
    Ok(())
}

async fn ensure_parent_exists(dav_fs: &fs::AsterDavFs, relative: &str) -> Result<(), HttpResponse> {
    let Some(parent) = parent_relative_path(relative) else {
        return Err(HttpResponse::MethodNotAllowed().finish());
    };
    if parent == "/" {
        return Ok(());
    }
    let parent_path = DavPath::new(&parent).map_err(|_| HttpResponse::BadRequest().finish())?;
    match dav_fs.metadata(&parent_path).await {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(HttpResponse::Conflict().finish()),
        Err(FsError::NotFound) => Err(HttpResponse::Conflict().finish()),
        Err(err) => Err(fs_error_response(err)),
    }
}

fn same_resource_path(left: &str, right: &str) -> bool {
    resource_identity_path(left) == resource_identity_path(right)
}

fn is_descendant_path(parent: &str, child: &str) -> bool {
    let parent = resource_identity_path(parent);
    let child = resource_identity_path(child);
    if parent == "/" || parent == child {
        return false;
    }
    let parent_prefix = format!("{parent}/");
    child.starts_with(&parent_prefix)
}

fn resource_identity_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_empty_body, is_descendant_path, same_resource_path};
    use actix_web::FromRequest;
    use actix_web::http::StatusCode;
    use actix_web::web;
    use bytes::Bytes;

    async fn payload_from_bytes(bytes: Bytes) -> web::Payload {
        let (req, mut dev_payload) = actix_web::test::TestRequest::default()
            .set_payload(bytes)
            .to_http_parts();
        web::Payload::from_request(&req, &mut dev_payload)
            .await
            .expect("test payload should extract")
    }

    #[actix_web::test]
    async fn ensure_empty_body_accepts_empty_payload() {
        let mut payload = payload_from_bytes(Bytes::new()).await;

        ensure_empty_body(&mut payload)
            .await
            .expect("empty MKCOL body should be accepted");
    }

    #[actix_web::test]
    async fn ensure_empty_body_ignores_empty_chunks() {
        let mut payload = payload_from_bytes(Bytes::new()).await;

        ensure_empty_body(&mut payload)
            .await
            .expect("empty MKCOL body chunks should be accepted");
    }

    #[actix_web::test]
    async fn ensure_empty_body_rejects_first_non_empty_chunk() {
        let mut payload = payload_from_bytes(Bytes::from_static(b"x")).await;

        let response = ensure_empty_body(&mut payload)
            .await
            .expect_err("non-empty MKCOL body should be rejected");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[actix_web::test]
    async fn ensure_empty_body_stops_after_first_non_empty_chunk() {
        let mut payload = payload_from_bytes(Bytes::from(vec![b'x'; 2 * 1024 * 1024])).await;

        let response = ensure_empty_body(&mut payload)
            .await
            .expect_err("large non-empty MKCOL body should be rejected");

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn resource_identity_ignores_collection_trailing_slash() {
        assert!(same_resource_path("/docs", "/docs/"));
        assert!(same_resource_path("/", "/"));
        assert!(!same_resource_path("/docs", "/docs/sub"));
    }

    #[test]
    fn descendant_identity_requires_path_boundary() {
        assert!(is_descendant_path("/docs", "/docs/sub"));
        assert!(is_descendant_path("/docs/", "/docs/sub/file.txt"));
        assert!(!is_descendant_path("/docs", "/docs"));
        assert!(!is_descendant_path("/docs", "/docs2/sub"));
        assert!(!is_descendant_path("/", "/docs"));
    }
}
