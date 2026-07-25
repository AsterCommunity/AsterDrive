//! WebDAV resource mutation handlers: MKCOL, DELETE, COPY, MOVE.

use std::collections::HashMap;

use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse};
use aster_forge_webdav::{
    DavCopyMoveMethod, DavMutationFailure, DavMutationPlanError, DavRequestHead, DavResourceKind,
    Depth, collection_created_response, delete_success_response, mutation_multistatus_response,
    mutation_plan_error_response, mutation_success_response, plan_copy_move_request,
    resource_identity_path, validate_collection_create_target, validate_delete_target,
};
use futures::{StreamExt, pin_mut};

use crate::webdav::{
    backend, child_relative_path, ensure_system_file_name_allowed, fs_error_response, responses,
    system_file,
};
use aster_forge_webdav::{DavFileSystem, DavLockSystem, DavPath, FsError, ReadDirMeta};

struct DavChild {
    path: DavPath,
    relative: String,
    is_dir: bool,
}

struct PartialMutationOutcome {
    failures: Vec<DavMutationFailure>,
    destination_exists: bool,
}

struct PartialMutationContext<'a> {
    dav_fs: &'a backend::AsterDavFs,
    lock_system: &'a dyn DavLockSystem,
    request_head: &'a DavRequestHead,
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
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
) -> HttpResponse {
    let path = request_head.target.clone();
    let relative = path.as_str().to_owned();
    if let Err(error) = validate_collection_create_target(&relative) {
        return aster_forge_webdav::actix::into_response(mutation_plan_error_response(error));
    }
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &relative) {
        return resp;
    }

    if let Err(response) = aster_forge_webdav::enforce_parent_collection(dav_fs, &path).await {
        return aster_forge_webdav::actix::into_response(response);
    }
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
        lock_system,
        &path,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }

    match dav_fs.create_dir(&path).await {
        Ok(()) => match collection_created_response(prefix, &path) {
            Ok(response) => aster_forge_webdav::actix::into_response(response),
            Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
        },
        Err(FsError::Exists) => aster_forge_webdav::actix::into_response(
            mutation_plan_error_response(DavMutationPlanError::MethodNotAllowed),
        ),
        Err(FsError::NotFound) => aster_forge_webdav::actix::into_response(
            mutation_plan_error_response(DavMutationPlanError::Conflict),
        ),
        Err(err) => fs_error_response(err),
    }
}

pub(crate) async fn handle_delete(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
) -> HttpResponse {
    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let path = request_head.target.clone();

    let meta = match dav_fs.metadata(&path).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    let resource_kind = if meta.is_dir() {
        DavResourceKind::Collection
    } else {
        DavResourceKind::File
    };
    if let Err(error) = validate_delete_target(resource_kind, depth) {
        return aster_forge_webdav::actix::into_response(mutation_plan_error_response(error));
    }
    if let Err(resp) = aster_forge_webdav::actix::evaluate_http_etag_preconditions(
        req.headers(),
        true,
        meta.etag().as_deref(),
        false,
    ) {
        return resp;
    }
    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &path,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if meta.is_dir() {
        if let Some(resp) =
            locked_multi_status_response(lock_system, &path, true, prefix, request_head).await
        {
            return resp;
        }
    } else if let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
        lock_system,
        &path,
        false,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
        lock_system,
        &path,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
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
            if let Err(error) = lock_system.delete(&path).await {
                tracing::warn!(
                    path = %path.as_str(),
                    error = ?error,
                    "failed to delete WebDAV locks after resource deletion"
                );
            }
            aster_forge_webdav::actix::into_response(delete_success_response())
        }
        Err(err) => fs_error_response(err),
    }
}

pub(crate) async fn handle_copy_move(
    req: &HttpRequest,
    request_head: &DavRequestHead,
    dav_fs: &backend::AsterDavFs,
    lock_system: &dyn DavLockSystem,
    prefix: &str,
    system_file_policy: &system_file::SystemFileBlockPolicy,
    is_move: bool,
) -> HttpResponse {
    let Some(depth) = request_head.depth else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let source = request_head.target.clone();
    let source_relative = source.as_str().to_owned();

    let request_scheme = request_head.origin.scheme.as_str();
    let request_host = request_head.origin.host.as_str();
    let Some(destination) = request_head.destination.as_ref() else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let destination_relative = destination.relative.clone();
    let destination = destination.path.clone();
    if let Err(resp) = ensure_system_file_name_allowed(system_file_policy, &destination_relative) {
        return resp;
    }
    if let Err(response) = aster_forge_webdav::enforce_parent_collection(dav_fs, &destination).await
    {
        return aster_forge_webdav::actix::into_response(response);
    }

    let source_meta = match dav_fs.metadata(&source).await {
        Ok(meta) => meta,
        Err(err) => return fs_error_response(err),
    };
    if let Err(resp) = aster_forge_webdav::actix::evaluate_http_etag_preconditions(
        req.headers(),
        true,
        source_meta.etag().as_deref(),
        false,
    ) {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_if_header_with_backends(
        request_head.if_header.as_ref(),
        dav_fs,
        lock_system,
        &source,
        prefix,
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }
    if is_move
        && let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
            lock_system,
            &source,
            false,
            prefix,
            request_head.if_header.as_ref(),
            request_scheme,
            request_host,
        )
        .await
    {
        return resp;
    }
    if is_move
        && let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
            lock_system,
            &source,
            prefix,
            request_head.if_header.as_ref(),
            request_scheme,
            request_host,
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
    let Some(overwrite) = request_head.overwrite else {
        return responses::empty(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let destination_is_collection = destination_meta.as_ref().is_some_and(|meta| meta.is_dir());
    let source_kind = if source_meta.is_dir() {
        DavResourceKind::Collection
    } else {
        DavResourceKind::File
    };
    let destination_kind = destination_meta.as_ref().map(|meta| {
        if meta.is_dir() {
            DavResourceKind::Collection
        } else {
            DavResourceKind::File
        }
    });
    let method = if is_move {
        DavCopyMoveMethod::Move
    } else {
        DavCopyMoveMethod::Copy
    };
    let plan = match plan_copy_move_request(
        method,
        depth,
        source_kind,
        destination_kind,
        &source_relative,
        &destination_relative,
        overwrite,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return aster_forge_webdav::actix::into_response(mutation_plan_error_response(error));
        }
    };
    if !plan.destination_deep
        && let Err(resp) = aster_forge_webdav::actix::enforce_unlocked(
            lock_system,
            &destination,
            false,
            prefix,
            request_head.if_header.as_ref(),
            request_scheme,
            request_host,
        )
        .await
    {
        return resp;
    }
    if let Err(resp) = aster_forge_webdav::actix::enforce_parent_unlocked(
        lock_system,
        &destination,
        prefix,
        request_head.if_header.as_ref(),
        request_scheme,
        request_host,
    )
    .await
    {
        return resp;
    }

    if plan.recursive_collection {
        let source_conflicts = if is_move {
            aster_forge_webdav::unsubmitted_lock_conflicts(
                lock_system,
                &source,
                true,
                prefix,
                request_head.if_header.as_ref(),
                &request_head.origin.scheme,
                &request_head.origin.host,
            )
            .await
        } else {
            Vec::new()
        };
        let destination_conflicts = aster_forge_webdav::unsubmitted_lock_conflicts(
            lock_system,
            &destination,
            true,
            prefix,
            request_head.if_header.as_ref(),
            &request_head.origin.scheme,
            &request_head.origin.host,
        )
        .await;
        if !source_conflicts.is_empty() || !destination_conflicts.is_empty() {
            let ctx = PartialMutationContext {
                dav_fs,
                lock_system,
                request_head,
                prefix,
                is_move,
            };
            let root = PartialMutationNode {
                source,
                source_relative,
                destination,
                destination_relative,
            };
            let outcome = partial_recursive_copy_move(
                &ctx,
                root,
                destination_exists,
                destination_is_collection,
            )
            .await;
            if !outcome.failures.is_empty() {
                return mutation_failure_response(prefix, &outcome.failures);
            }
            return aster_forge_webdav::actix::into_response(mutation_success_response(
                outcome.destination_exists,
            ));
        }
    }

    if plan.destination_deep
        && let Some(resp) =
            locked_multi_status_response(lock_system, &destination, true, prefix, request_head)
                .await
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
            if is_move && let Err(error) = lock_system.delete(&source).await {
                tracing::warn!(path = %source_relative, error = ?error, "failed to delete WebDAV locks after move");
            }
            aster_forge_webdav::actix::into_response(mutation_success_response(destination_exists))
        }
        Err(err) => fs_error_response(err),
    }
}

async fn locked_multi_status_response(
    lock_system: &dyn DavLockSystem,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    request_head: &DavRequestHead,
) -> Option<HttpResponse> {
    let conflicts = aster_forge_webdav::unsubmitted_lock_conflicts(
        lock_system,
        path,
        deep,
        prefix,
        request_head.if_header.as_ref(),
        &request_head.origin.scheme,
        &request_head.origin.host,
    )
    .await;
    if conflicts.is_empty() {
        return None;
    }

    Some(multi_status_locked_response(prefix, &conflicts))
}

fn multi_status_locked_response(
    prefix: &str,
    locks: &[aster_forge_webdav::DavLock],
) -> HttpResponse {
    let failures = locks
        .iter()
        .map(|lock| DavMutationFailure::locked((*lock.path).clone(), (*lock.path).clone()))
        .collect::<Vec<_>>();
    mutation_failure_response(prefix, &failures)
}

async fn partial_recursive_copy_move(
    ctx: &PartialMutationContext<'_>,
    root: PartialMutationNode,
    destination_exists: bool,
    destination_is_collection: bool,
) -> PartialMutationOutcome {
    let mut failures = Vec::new();
    if destination_exists && !destination_is_collection {
        let conflicts = collect_lock_failures(ctx, &root.destination, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(&mut failures, conflicts);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
        if let Err(error) = ctx.dav_fs.remove_file(&root.destination).await {
            push_fs_failure(&mut failures, &root.destination, error);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
        if let Err(error) = ctx
            .dav_fs
            .copy_dir_shallow(&root.source, &root.destination)
            .await
        {
            push_fs_failure(&mut failures, &root.destination, error);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
    } else if destination_exists && destination_is_collection {
        let conflicts = collect_lock_failures(ctx, &root.destination, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(&mut failures, conflicts);
            return PartialMutationOutcome {
                failures,
                destination_exists,
            };
        }
    }

    if !destination_exists
        && let Err(error) = ctx
            .dav_fs
            .copy_dir_shallow(&root.source, &root.destination)
            .await
    {
        push_fs_failure(&mut failures, &root.destination, error);
        return PartialMutationOutcome {
            failures,
            destination_exists,
        };
    }

    let mut work = Vec::new();
    work.push(PartialMutationWork::FinalizeDirectory(root.clone()));
    if !push_directory_children(ctx, &root, &mut work, &mut failures).await {
        work.pop();
    }

    while let Some(work_item) = work.pop() {
        match work_item {
            PartialMutationWork::ProcessFile(node) => {
                partial_copy_move_file(ctx, &node, &mut failures).await;
            }
            PartialMutationWork::VisitDirectory(node) => {
                let dest_meta = match ctx.dav_fs.metadata(&node.destination).await {
                    Ok(meta) => Some(meta),
                    Err(FsError::NotFound) => None,
                    Err(error) => {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                };
                if dest_meta.as_ref().is_some_and(|meta| !meta.is_dir()) {
                    let conflicts = collect_lock_failures(ctx, &node.destination, false).await;
                    if !conflicts.is_empty() {
                        extend_unique_failures(&mut failures, conflicts);
                        continue;
                    }
                    if let Err(error) = ctx.dav_fs.remove_file(&node.destination).await {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                    if let Err(error) = ctx
                        .dav_fs
                        .copy_dir_shallow(&node.source, &node.destination)
                        .await
                    {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                } else if dest_meta.as_ref().is_some_and(|meta| meta.is_dir()) {
                    let conflicts = collect_lock_failures(ctx, &node.destination, false).await;
                    if !conflicts.is_empty() {
                        extend_unique_failures(&mut failures, conflicts);
                        continue;
                    }
                } else {
                    if let Err(error) = ctx
                        .dav_fs
                        .copy_dir_shallow(&node.source, &node.destination)
                        .await
                    {
                        push_fs_failure(&mut failures, &node.destination, error);
                        continue;
                    }
                }

                work.push(PartialMutationWork::FinalizeDirectory(node.clone()));
                if !push_directory_children(ctx, &node, &mut work, &mut failures).await {
                    work.pop();
                }
            }
            PartialMutationWork::FinalizeDirectory(node) if ctx.is_move => {
                let conflicts = collect_lock_failures(ctx, &node.source, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                let remaining =
                    match collect_children(ctx.dav_fs, &node.source, &node.source_relative).await {
                        Ok(remaining) => remaining,
                        Err(error) => {
                            push_fs_failure(&mut failures, &node.source, error);
                            continue;
                        }
                    };
                if remaining.is_empty() {
                    if let Err(error) = ctx.dav_fs.remove_dir(&node.source).await {
                        push_fs_failure(&mut failures, &node.source, error);
                        continue;
                    }
                    if let Err(error) = ctx.lock_system.delete(&node.source).await {
                        tracing::warn!(path = %node.source_relative, error = ?error, "failed to delete WebDAV locks after partial move");
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
                if let Err(error) = ctx.dav_fs.remove_file(&node.path).await {
                    push_fs_failure(&mut failures, &node.path, error);
                }
            }
            PartialMutationWork::VisitDestinationDirectory(node) => {
                work.push(PartialMutationWork::FinalizeDestinationDirectory(
                    node.clone(),
                ));
                if !push_destination_children(ctx, &node, &mut work, &mut failures).await {
                    work.pop();
                }
            }
            PartialMutationWork::FinalizeDestinationDirectory(node) => {
                let conflicts = collect_lock_failures(ctx, &node.path, false).await;
                if !conflicts.is_empty() {
                    extend_unique_failures(&mut failures, conflicts);
                    continue;
                }
                let remaining = match collect_children(ctx.dav_fs, &node.path, &node.relative).await
                {
                    Ok(remaining) => remaining,
                    Err(error) => {
                        push_fs_failure(&mut failures, &node.path, error);
                        continue;
                    }
                };
                if remaining.is_empty() {
                    if let Err(error) = ctx.dav_fs.remove_dir(&node.path).await {
                        push_fs_failure(&mut failures, &node.path, error);
                        continue;
                    }
                    if let Err(error) = ctx.lock_system.delete(&node.path).await {
                        tracing::warn!(path = %node.relative, error = ?error, "failed to delete WebDAV locks after destination overwrite");
                    }
                }
            }
        }
    }

    PartialMutationOutcome {
        failures,
        destination_exists,
    }
}

async fn push_directory_children(
    ctx: &PartialMutationContext<'_>,
    node: &PartialMutationNode,
    work: &mut Vec<PartialMutationWork>,
    failures: &mut Vec<DavMutationFailure>,
) -> bool {
    let children = match collect_children(ctx.dav_fs, &node.source, &node.source_relative).await {
        Ok(children) => children,
        Err(error) => {
            push_fs_failure(failures, &node.source, error);
            return false;
        }
    };
    let mut destination_nodes = HashMap::with_capacity(children.len());
    let mut source_work = Vec::with_capacity(children.len());
    for child in children {
        let dest_relative = aster_forge_webdav::replace_relative_prefix(
            &child.relative,
            &node.source_relative,
            &node.destination_relative,
        );
        let dest_path = match DavPath::new(&dest_relative) {
            Ok(path) => path,
            Err(_) => {
                push_fs_failure(failures, &node.source, FsError::BadRequest);
                return false;
            }
        };
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
        match collect_children(ctx.dav_fs, &node.destination, &node.destination_relative).await {
            Ok(children) => children,
            Err(error) => {
                push_fs_failure(failures, &node.destination, error);
                return true;
            }
        };
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
    true
}

async fn push_destination_children(
    ctx: &PartialMutationContext<'_>,
    node: &DestinationMutationNode,
    work: &mut Vec<PartialMutationWork>,
    failures: &mut Vec<DavMutationFailure>,
) -> bool {
    let children = match collect_children(ctx.dav_fs, &node.path, &node.relative).await {
        Ok(children) => children,
        Err(error) => {
            push_fs_failure(failures, &node.path, error);
            return false;
        }
    };
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
    true
}

async fn partial_copy_move_file(
    ctx: &PartialMutationContext<'_>,
    node: &PartialMutationNode,
    failures: &mut Vec<DavMutationFailure>,
) {
    if ctx.is_move {
        let conflicts = collect_lock_failures(ctx, &node.source, false).await;
        if !conflicts.is_empty() {
            extend_unique_failures(failures, conflicts);
            return;
        }
    }
    let destination_is_collection = match ctx.dav_fs.metadata(&node.destination).await {
        Ok(meta) => meta.is_dir(),
        Err(FsError::NotFound) => false,
        Err(error) => {
            push_fs_failure(failures, &node.destination, error);
            return;
        }
    };
    let dest_conflicts =
        collect_lock_failures(ctx, &node.destination, destination_is_collection).await;
    if !dest_conflicts.is_empty() {
        extend_unique_failures(failures, dest_conflicts);
        return;
    }
    if ctx.is_move {
        if let Err(error) = ctx.dav_fs.rename(&node.source, &node.destination).await {
            push_fs_failure(failures, &node.destination, error);
            return;
        }
        if let Err(error) = ctx.lock_system.delete(&node.source).await {
            tracing::warn!(path = %node.source.as_str(), error = ?error, "failed to delete WebDAV locks after partial file move");
        }
    } else if let Err(error) = ctx.dav_fs.copy(&node.source, &node.destination).await {
        push_fs_failure(failures, &node.destination, error);
    }
}

fn push_fs_failure(failures: &mut Vec<DavMutationFailure>, path: &DavPath, error: FsError) {
    let status = match error {
        FsError::NotFound => StatusCode::NOT_FOUND,
        FsError::Forbidden => StatusCode::FORBIDDEN,
        FsError::Exists => StatusCode::CONFLICT,
        FsError::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
        FsError::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        FsError::BadRequest => StatusCode::BAD_REQUEST,
        FsError::GeneralFailure => StatusCode::INTERNAL_SERVER_ERROR,
    };
    extend_unique_failures(
        failures,
        [DavMutationFailure::status(path.clone(), status.as_u16())],
    );
}

fn extend_unique_failures(
    failures: &mut Vec<DavMutationFailure>,
    additions: impl IntoIterator<Item = DavMutationFailure>,
) {
    for failure in additions {
        if failures.contains(&failure) {
            continue;
        }
        failures.push(failure);
    }
}

async fn collect_lock_failures(
    ctx: &PartialMutationContext<'_>,
    path: &DavPath,
    deep: bool,
) -> Vec<DavMutationFailure> {
    aster_forge_webdav::unsubmitted_lock_conflicts(
        ctx.lock_system,
        path,
        deep,
        ctx.prefix,
        ctx.request_head.if_header.as_ref(),
        &ctx.request_head.origin.scheme,
        &ctx.request_head.origin.host,
    )
    .await
    .into_iter()
    .map(|lock| DavMutationFailure::locked((*lock.path).clone(), (*lock.path).clone()))
    .collect()
}

async fn collect_children(
    dav_fs: &backend::AsterDavFs,
    path: &DavPath,
    relative: &str,
) -> Result<Vec<DavChild>, FsError> {
    let entries = dav_fs.read_dir(path, ReadDirMeta::Data).await?;
    pin_mut!(entries);
    let mut children = Vec::new();
    while let Some(entry) = entries.next().await {
        let entry = entry?;
        let meta = entry.metadata().await?;
        let child_relative = child_relative_path(relative, &entry.name(), meta.is_dir())
            .map_err(|_| FsError::GeneralFailure)?;
        let child_path = DavPath::new(&child_relative).map_err(|_| FsError::GeneralFailure)?;
        children.push(DavChild {
            path: child_path,
            relative: child_relative,
            is_dir: meta.is_dir(),
        });
    }
    Ok(children)
}

fn mutation_failure_response(prefix: &str, failures: &[DavMutationFailure]) -> HttpResponse {
    match mutation_multistatus_response(prefix, failures) {
        Ok(response) => aster_forge_webdav::actix::into_response(response),
        Err(_) => responses::empty(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use super::push_fs_failure;
    use actix_web::http::StatusCode;
    use aster_forge_webdav::{DavMutationFailure, DavPath, FsError};

    #[test]
    fn recursive_mutation_fs_errors_keep_resource_statuses() {
        let path = DavPath::new("/failed.txt").unwrap();
        let cases = [
            (FsError::NotFound, StatusCode::NOT_FOUND),
            (FsError::Forbidden, StatusCode::FORBIDDEN),
            (FsError::Exists, StatusCode::CONFLICT),
            (
                FsError::InsufficientStorage,
                StatusCode::INSUFFICIENT_STORAGE,
            ),
            (FsError::TooLarge, StatusCode::PAYLOAD_TOO_LARGE),
            (FsError::BadRequest, StatusCode::BAD_REQUEST),
            (FsError::GeneralFailure, StatusCode::INTERNAL_SERVER_ERROR),
        ];

        for (error, status) in cases {
            let mut failures = Vec::new();
            push_fs_failure(&mut failures, &path, error);
            assert_eq!(
                failures,
                vec![DavMutationFailure::status(path.clone(), status.as_u16())]
            );
        }
    }
}
