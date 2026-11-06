//! 将当前 canonical 锁根投影到本账号挂载路径。存储的 lockroot_path 仅是历史呈现，
//! 不能用于跨账号授权、路径冲突或锁发现：两个挂载根都可能存在 `/same.txt`。

use super::*;
use crate::services::workspace::scope::{ensure_file_scope, ensure_folder_scope};

/// namespace 先限定 workspace，实体归属再防止损坏/漂移的锁根跨空间出现。
/// root 限制依据当前实体 ancestry 对应的真实路径计算；返回 None 的锁不向该挂载
/// 暴露 token。调用方不能把这个可见性过滤代替 mutation 的 canonical 锁约束。
pub(super) async fn project_lock_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    lock: &resource_lock::Model,
) -> crate::errors::Result<Option<resource_lock::Model>> {
    use aster_drive_model::types::LockRootKind;
    let full_path = match lock.root_kind {
        LockRootKind::WorkspaceRoot => match root_folder_id {
            Some(_) => return Ok(None),
            None => {
                return Ok(Some(resource_lock::Model {
                    lockroot_path: Some("/".to_owned()),
                    ..lock.clone()
                }));
            }
        },
        LockRootKind::File => {
            let id = lock.root_file_id.ok_or_else(|| {
                crate::errors::AsterError::internal_error("file lock missing canonical root")
            })?;
            let file = file_repo::find_by_id(db, id).await?;
            ensure_file_scope(&file, scope)?;
            crate::services::files::lock::resolve_entity_path(db, EntityType::File, id).await?
        }
        LockRootKind::Folder => {
            let id = lock.root_folder_id.ok_or_else(|| {
                crate::errors::AsterError::internal_error("folder lock missing canonical root")
            })?;
            let folder = folder_repo::find_by_id(db, id).await?;
            ensure_folder_scope(&folder, scope)?;
            crate::services::files::lock::resolve_entity_path(db, EntityType::Folder, id).await?
        }
    };
    let prefix = match root_folder_id {
        Some(id) => {
            let root = folder_repo::find_by_id(db, id).await?;
            ensure_folder_scope(&root, scope)?;
            crate::services::files::lock::resolve_entity_path(db, EntityType::Folder, id)
                .await?
                .trim_end_matches('/')
                .to_owned()
        }
        None => match scope {
            WorkspaceStorageScope::Personal { .. } => String::new(),
            WorkspaceStorageScope::Team { team_id, .. } => format!("/teams/{team_id}"),
        },
    };
    Ok(project_relative_path(lock, &full_path, &prefix))
}

/// 路径只用于输出投影。目录必须保留尾斜杠，才能保持 tagged If 的 collection URI
/// 语义；strip_prefix 后再检查分隔符，防止 /root-other 被误当成 /root 的后代。
fn project_relative_path(
    lock: &resource_lock::Model,
    full_path: &str,
    prefix: &str,
) -> Option<resource_lock::Model> {
    let relative = match full_path.trim_end_matches('/').strip_prefix(prefix) {
        Some("") => "/".to_owned(),
        Some(relative) if relative.starts_with('/') => relative.to_owned(),
        _ => return None,
    };
    let relative = match lock.root_kind {
        aster_drive_model::types::LockRootKind::Folder if relative != "/" => {
            format!("{}/", relative.trim_end_matches('/'))
        }
        _ => relative,
    };
    Some(resource_lock::Model {
        lockroot_path: Some(relative),
        ..lock.clone()
    })
}

pub(super) async fn projected_locks_on<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
) -> crate::errors::Result<Vec<resource_lock::Model>> {
    use aster_drive_model::types::LockRootKind;
    let locks = lock_repo::find_all_by_namespace(db, namespace_id).await?;
    let mut file_ids: Vec<_> = locks.iter().filter_map(|lock| lock.root_file_id).collect();
    file_ids.sort_unstable();
    file_ids.dedup();
    let files = file_repo::find_by_ids(db, &file_ids).await?;
    let files: HashMap<_, _> = files.into_iter().map(|file| (file.id, file)).collect();
    let mut folder_ids: Vec<_> = locks
        .iter()
        .filter_map(|lock| lock.root_folder_id)
        .chain(files.values().filter_map(|file| file.folder_id))
        .chain(root_folder_id)
        .collect();
    folder_ids.sort_unstable();
    folder_ids.dedup();
    let folders = folder_repo::find_by_ids(db, &folder_ids).await?;
    let folders: HashMap<_, _> = folders
        .into_iter()
        .map(|folder| (folder.id, folder))
        .collect();
    // 一次批量加载所有锁根与 ancestor frontier。查询次数跟最大目录深度/批次增长，
    // 不再对每个锁重复查 file、重建同一个 mount root 和逐级走目录链。
    let paths = crate::services::files::folder::build_folder_paths(db, &folder_ids).await?;
    let team_prefix = match scope {
        WorkspaceStorageScope::Personal { .. } => String::new(),
        WorkspaceStorageScope::Team { team_id, .. } => format!("/teams/{team_id}"),
    };
    let prefix = match root_folder_id {
        Some(root_id) => {
            let root = folders.get(&root_id).ok_or_else(|| {
                crate::errors::AsterError::folder_not_found("mounted root unavailable")
            })?;
            ensure_folder_scope(root, scope)?;
            let path = paths.get(&root_id).ok_or_else(|| {
                crate::errors::AsterError::folder_not_found("mounted root path unavailable")
            })?;
            format!("{team_prefix}{}", path.trim_end_matches('/'))
        }
        None => team_prefix.clone(),
    };
    let mut projected = Vec::new();
    for lock in locks {
        if lock.timeout_at.is_some_and(|expires| expires <= Utc::now()) {
            continue;
        }
        let path = match lock.root_kind {
            LockRootKind::WorkspaceRoot => match root_folder_id {
                Some(_) => continue,
                None => {
                    projected.push(resource_lock::Model {
                        lockroot_path: Some("/".to_owned()),
                        ..lock
                    });
                    continue;
                }
            },
            LockRootKind::File => {
                let file = lock
                    .root_file_id
                    .and_then(|id| files.get(&id))
                    .ok_or_else(|| {
                        crate::errors::AsterError::file_not_found("lock root unavailable")
                    })?;
                ensure_file_scope(file, scope)?;
                let parent = match file.folder_id {
                    Some(id) => paths
                        .get(&id)
                        .ok_or_else(|| {
                            crate::errors::AsterError::folder_not_found("lock parent unavailable")
                        })?
                        .trim_end_matches('/'),
                    None => "",
                };
                format!("{team_prefix}{parent}/{}", file.name)
            }
            LockRootKind::Folder => {
                let id = lock.root_folder_id.ok_or_else(|| {
                    crate::errors::AsterError::internal_error("folder lock missing root")
                })?;
                let folder = folders.get(&id).ok_or_else(|| {
                    crate::errors::AsterError::folder_not_found("lock root unavailable")
                })?;
                ensure_folder_scope(folder, scope)?;
                let path = paths.get(&id).ok_or_else(|| {
                    crate::errors::AsterError::folder_not_found("lock root path unavailable")
                })?;
                format!("{team_prefix}{}/", path.trim_end_matches('/'))
            }
        };
        if let Some(lock) = project_relative_path(&lock, &path, &prefix) {
            projected.push(lock)
        }
    }
    projected.sort_by_key(|lock| lock.id);
    Ok(projected)
}

/// 收集已经满足的、与本挂载可见锁根绑定的正向 token 条件。产品层仍验证这些
/// token 对实际受影响 canonical target 的约束，协议层负责 If/URI 条件语义。
pub(crate) async fn submitted_credentials_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    conditions: &super::super::DavMutationConditions<'_>,
) -> crate::errors::Result<LockMutationCredentials> {
    let namespace = match find_lock_namespace(db, scope).await? {
        Some(namespace) => namespace,
        None => return Ok(LockMutationCredentials::None),
    };
    let mut tokens = Vec::new();
    for lock in projected_locks_on(db, namespace.id, scope, root_folder_id).await? {
        let href = href_for_relative(conditions.prefix, lock.path());
        let submitted = conditions.if_header.map_or_else(Vec::new, |header| {
            submitted_lock_tokens(
                header,
                &href,
                conditions.request_scheme,
                conditions.request_host,
            )
        });
        if submitted.iter().any(|token| token == &lock.token) {
            tokens.push(lock.token)
        }
    }
    Ok(LockMutationCredentials::SubmittedTokens(tokens))
}
