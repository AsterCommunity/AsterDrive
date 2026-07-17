//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use std::collections::HashMap;

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
    request_origin, request_path, responses, status_element, system_file, text_element,
    xml_response,
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

#[derive(Clone)]
struct PartialMutationNode {
    source: DavPath,
    source_relative: String,
    destination: DavPath,
    destination_relative: String,
}

#[derive(Clone)]
struct DestinationMutationNode {
    path: DavPath,
    relative: String,
}

enum PartialMutationWork {
    VisitDirectory(PartialMutationNode),
    ProcessFile(PartialMutationNode),
    FinalizeDirectory(PartialMutationNode),
    RemoveDestinationFile(DestinationMutationNode),
    VisitDestinationDirectory(DestinationMutationNode),
    FinalizeDestinationDirectory(DestinationMutationNode),
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
        return responses::empty(StatusCode::METHOD_NOT_ALLOWED);
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
        Err(FsError::Exists) => responses::empty(StatusCode::METHOD_NOT_ALLOWED),
        Err(FsError::NotFound) => responses::conflict(),
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
        return responses::bad_request();
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
        return responses::forbidden();
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &destination_relative) {
        return resp;
    }
    if let Err(resp) = ensure_parent_exists(dav_fs, &destination_relative).await {
        return resp;
    }

    let destination = match DavPath::new(&destination_relative) {
        Ok(path) => path,
        Err(_) => return responses::bad_request_text("Invalid destination path"),
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
            return responses::bad_request();
        }
        if !is_move && depth == Depth::One {
            return responses::bad_request();
        }
    }
    let recursive_collection_copy_or_move =
        source_meta.is_dir() && (is_move || depth != Depth::Zero);
    if recursive_collection_copy_or_move
        && is_descendant_path(&source_relative, &destination_relative)
    {
        return responses::forbidden();
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
        return responses::precondition_failed();
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
                source,
                source_relative,
                destination,
                destination_relative,
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
    responses::no_store(status)
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

    responses::with_no_store(xml_response(multistatus, multi_status()))
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
    root: PartialMutationNode,
    destination_exists: bool,
    destination_is_collection: bool,
) -> Result<PartialMutationOutcome, FsError> {
    let mut failures = Vec::new();
    if destination_exists && !destination_is_collection {
        let conflicts = collect_lock_failures(ctx, &root.destination, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(&mut failures, conflicts);
            return Ok(PartialMutationOutcome {
                failures,
                destination_exists,
            });
        }
        ctx.dav_fs.remove_file(&root.destination).await?;
        ctx.dav_fs
            .copy_dir_shallow(&root.source, &root.destination)
            .await?;
    } else if destination_exists && destination_is_collection {
        let conflicts = collect_lock_failures(ctx, &root.destination, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(&mut failures, conflicts);
            return Ok(PartialMutationOutcome {
                failures,
                destination_exists,
            });
        }
    }

    if !destination_exists {
        ctx.dav_fs
            .copy_dir_shallow(&root.source, &root.destination)
            .await?;
    }

    let mut work = Vec::new();
    work.push(PartialMutationWork::FinalizeDirectory(root.clone()));
    push_directory_children(ctx, &root, &mut work).await?;

    while let Some(work_item) = work.pop() {
        match work_item {
            PartialMutationWork::ProcessFile(node) => {
                partial_copy_move_file(ctx, &node, &mut failures).await?;
            }
            PartialMutationWork::VisitDirectory(node) => {
                let dest_meta = match ctx.dav_fs.metadata(&node.destination).await {
                    Ok(meta) => Some(meta),
                    Err(FsError::NotFound) => None,
                    Err(err) => return Err(err),
                };
                if dest_meta.as_ref().is_some_and(|meta| !meta.is_dir()) {
                    let conflicts = collect_lock_failures(ctx, &node.destination, false).await;
                    if !conflicts.is_empty() {
                        extend_unique_failures(&mut failures, conflicts);
                        continue;
                    }
                    ctx.dav_fs.remove_file(&node.destination).await?;
                    ctx.dav_fs
                        .copy_dir_shallow(&node.source, &node.destination)
                        .await?;
                } else if dest_meta.as_ref().is_some_and(|meta| meta.is_dir()) {
                    let conflicts = collect_lock_failures(ctx, &node.destination, false).await;
                    if !conflicts.is_empty() {
                        extend_unique_failures(&mut failures, conflicts);
                        continue;
                    }
                } else {
                    ctx.dav_fs
                        .copy_dir_shallow(&node.source, &node.destination)
                        .await?;
                }

                work.push(PartialMutationWork::FinalizeDirectory(node.clone()));
                push_directory_children(ctx, &node, &mut work).await?;
            }
            PartialMutationWork::FinalizeDirectory(node) if ctx.is_move => {
                let conflicts = collect_lock_failures(ctx, &node.source, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                let remaining =
                    collect_children(ctx.dav_fs, &node.source, &node.source_relative).await?;
                if remaining.is_empty() {
                    ctx.dav_fs.remove_dir(&node.source).await?;
                    if ctx.lock_system.delete(&node.source).await.is_err() {
                        tracing::warn!(path = %node.source_relative, "failed to delete WebDAV locks after partial move");
                    }
                }
            }
            PartialMutationWork::FinalizeDirectory(_) => {}
            PartialMutationWork::RemoveDestinationFile(node) => {
                let conflicts = collect_lock_failures(ctx, &node.path, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                ctx.dav_fs.remove_file(&node.path).await?;
            }
            PartialMutationWork::VisitDestinationDirectory(node) => {
                work.push(PartialMutationWork::FinalizeDestinationDirectory(
                    node.clone(),
                ));
                push_destination_children(ctx, &node, &mut work).await?;
            }
            PartialMutationWork::FinalizeDestinationDirectory(node) => {
                let conflicts = collect_lock_failures(ctx, &node.path, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                let remaining = collect_children(ctx.dav_fs, &node.path, &node.relative).await?;
                if remaining.is_empty() {
                    ctx.dav_fs.remove_dir(&node.path).await?;
                    if ctx.lock_system.delete(&node.path).await.is_err() {
                        tracing::warn!(path = %node.relative, "failed to delete WebDAV locks after destination overwrite");
                    }
                }
            }
        }
    }

    Ok(PartialMutationOutcome {
        failures,
        destination_exists,
    })
}

async fn push_directory_children(
    ctx: &PartialMutationContext<'_>,
    node: &PartialMutationNode,
    work: &mut Vec<PartialMutationWork>,
) -> Result<(), FsError> {
    let children = collect_children(ctx.dav_fs, &node.source, &node.source_relative).await?;
    let mut destination_nodes = HashMap::with_capacity(children.len());
    let mut source_work = Vec::with_capacity(children.len());
    for child in children {
        let dest_relative = replace_relative_prefix(
            &child.relative,
            &node.source_relative,
            &node.destination_relative,
        );
        let dest_path = DavPath::new(&dest_relative).map_err(|_| FsError::BadRequest)?;
        let child_node = PartialMutationNode {
            source: child.path,
            source_relative: child.relative,
            destination: dest_path,
            destination_relative: dest_relative,
        };
        destination_nodes.insert(
            resource_identity_path(&child_node.destination_relative),
            child.is_dir,
        );
        source_work.push(if child.is_dir {
            PartialMutationWork::VisitDirectory(child_node)
        } else {
            PartialMutationWork::ProcessFile(child_node)
        });
    }
    for work_item in source_work.into_iter().rev() {
        work.push(work_item);
    }

    let destination_children =
        collect_children(ctx.dav_fs, &node.destination, &node.destination_relative).await?;
    for child in destination_children.into_iter().rev() {
        if destination_nodes.get(&resource_identity_path(&child.relative)) == Some(&child.is_dir) {
            continue;
        }
        let node = DestinationMutationNode {
            path: child.path,
            relative: child.relative,
        };
        work.push(if child.is_dir {
            PartialMutationWork::VisitDestinationDirectory(node)
        } else {
            PartialMutationWork::RemoveDestinationFile(node)
        });
    }
    Ok(())
}

async fn push_destination_children(
    ctx: &PartialMutationContext<'_>,
    node: &DestinationMutationNode,
    work: &mut Vec<PartialMutationWork>,
) -> Result<(), FsError> {
    let children = collect_children(ctx.dav_fs, &node.path, &node.relative).await?;
    for child in children.into_iter().rev() {
        let child_node = DestinationMutationNode {
            path: child.path,
            relative: child.relative,
        };
        work.push(if child.is_dir {
            PartialMutationWork::VisitDestinationDirectory(child_node)
        } else {
            PartialMutationWork::RemoveDestinationFile(child_node)
        });
    }
    Ok(())
}

async fn partial_copy_move_file(
    ctx: &PartialMutationContext<'_>,
    node: &PartialMutationNode,
    failures: &mut Vec<MultiStatusFailure>,
) -> Result<(), FsError> {
    if ctx.is_move {
        let conflicts = collect_lock_failures(ctx, &node.source, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(failures, conflicts);
            return Ok(());
        }
    }
    let destination_is_collection = match ctx.dav_fs.metadata(&node.destination).await {
        Ok(meta) => meta.is_dir(),
        Err(FsError::NotFound) => false,
        Err(err) => return Err(err),
    };
    let dest_conflicts =
        collect_lock_failures(ctx, &node.destination, destination_is_collection).await;
    if !dest_conflicts.is_empty() {
        extend_unique_failures(failures, dest_conflicts);
        return Ok(());
    }
    if ctx.is_move {
        ctx.dav_fs.rename(&node.source, &node.destination).await?;
        if ctx.lock_system.delete(&node.source).await.is_err() {
            tracing::warn!(path = %decoded_path_string(&node.source), "failed to delete WebDAV locks after partial file move");
        }
    } else {
        ctx.dav_fs.copy(&node.source, &node.destination).await?;
    }
    Ok(())
}

fn extend_unique_failures(
    failures: &mut Vec<MultiStatusFailure>,
    additions: impl IntoIterator<Item = MultiStatusFailure>,
) {
    for failure in additions {
        if failures.iter().any(|existing| {
            existing.path == failure.path
                && existing.status == failure.status
                && existing.lock_path == failure.lock_path
        }) {
            continue;
        }
        failures.push(failure);
    }
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
    let suffix = path
        .strip_prefix(source_prefix)
        .filter(|suffix| suffix.is_empty() || suffix.starts_with('/'))
        .unwrap_or(path);
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

    responses::with_no_store(xml_response(multistatus, multi_status()))
}

pub(crate) async fn ensure_empty_body(payload: &mut web::Payload) -> Result<(), HttpResponse> {
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_err(|_| responses::request_body_read_error())?;
        if !chunk.is_empty() {
            return Err(responses::unsupported_media_type());
        }
    }
    Ok(())
}

async fn ensure_parent_exists(dav_fs: &fs::AsterDavFs, relative: &str) -> Result<(), HttpResponse> {
    let Some(parent) = parent_relative_path(relative) else {
        return Err(responses::empty(StatusCode::METHOD_NOT_ALLOWED));
    };
    if parent == "/" {
        return Ok(());
    }
    let parent_path = DavPath::new(&parent).map_err(|_| responses::bad_request())?;
    match dav_fs.metadata(&parent_path).await {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(responses::conflict()),
        Err(FsError::NotFound) => Err(responses::conflict()),
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
    use super::{
        ensure_empty_body, is_descendant_path, replace_relative_prefix, same_resource_path,
    };
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

    #[test]
    fn replace_relative_prefix_strips_only_one_source_prefix() {
        assert_eq!(
            replace_relative_prefix("/docs/docs/file.txt", "/docs", "/archive"),
            "/archive/docs/file.txt"
        );
    }

    #[test]
    fn replace_relative_prefix_handles_collection_root_and_trailing_slashes() {
        assert_eq!(
            replace_relative_prefix("/docs/", "/docs/", "/archive/"),
            "/archive/"
        );
        assert_eq!(
            replace_relative_prefix("/docs/sub/", "/docs/", "/archive/"),
            "/archive/sub/"
        );
    }

    #[test]
    fn replace_relative_prefix_leaves_unmatched_paths_attached_to_destination_prefix() {
        assert_eq!(
            replace_relative_prefix("/docs2/file.txt", "/docs", "/archive"),
            "/archive/docs2/file.txt"
        );
    }
}
