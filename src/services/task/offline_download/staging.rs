//! Offline engines own fresh output directories; import owns a sealed inode.
//!
//! Slow download/hash/upload I/O remains outside this boundary. Preparing a directory
//! and sealing an output use only bounded filesystem metadata operations and one rename.

use std::fmt;
use std::fs::File;
use std::io;
#[cfg(unix)]
use std::path::Component;
use std::path::{Path, PathBuf};

use aster_forge_utils::raii::TempDirGuard;

use crate::errors::{AsterError, Result};

const SOURCE_NAME: &str = "source";

/// One engine attempt, never reused by a fallback engine.
pub(super) struct DownloadAttempt {
    source_path: PathBuf,
    parent_path: PathBuf,
    guard: TempDirGuard,
    #[cfg(unix)]
    parent: File,
    #[cfg(unix)]
    directory: File,
    #[cfg(unix)]
    shared_group: Option<u32>,
}

impl fmt::Debug for DownloadAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DownloadAttempt")
            .finish_non_exhaustive()
    }
}

impl DownloadAttempt {
    pub(super) fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// Open the completed output once, move that inode into private import staging,
    /// and return that same handle for hashing. No whole-file copy is performed.
    pub(super) async fn seal(self) -> Result<SealedDownload> {
        tokio::task::spawn_blocking(move || seal(self))
            .await
            .map_err(|error| staging_error(io::Error::other(error)))?
            .map_err(staging_error)
    }
}

/// The private path remains owned until publication/rollback has finished.
pub(super) struct SealedDownload {
    path: PathBuf,
    file: tokio::fs::File,
    _guard: TempDirGuard,
}

impl fmt::Debug for SealedDownload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedDownload")
            .finish_non_exhaustive()
    }
}

impl SealedDownload {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn file_mut(&mut self) -> &mut tokio::fs::File {
        &mut self.file
    }
}

pub(super) async fn prepare_attempt(task_dir: &Path, shared: bool) -> Result<DownloadAttempt> {
    let task_dir = task_dir.to_path_buf();
    tokio::task::spawn_blocking(move || prepare(&task_dir, shared))
        .await
        .map_err(|error| staging_error(io::Error::other(error)))?
        .map_err(staging_error)
}

/// Builtin output is exclusively created, never truncating a prior pathname.
pub(super) async fn create_source(path: &Path) -> Result<tokio::fs::File> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || create_file(&path).map(tokio::fs::File::from_std))
        .await
        .map_err(|error| staging_error(io::Error::other(error)))?
        .map_err(staging_error)
}

fn staging_error(error: io::Error) -> AsterError {
    AsterError::storage_driver_error(format!("offline download staging rejected: {error}"))
}

fn denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

#[cfg(unix)]
mod unix {
    use super::*;
    use rustix::fs::{
        AtFlags, FileType, Mode, OFlags, fchmod, fchown, fstat, mkdirat, open, openat, renameat,
        statat, unlinkat,
    };
    use rustix::process::{Gid, geteuid};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn open_directory(path: &Path) -> io::Result<File> {
        Ok(File::from(open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?))
    }

    fn verify_directory(
        file: &File,
        shared_group: Option<u32>,
        owner_only: bool,
    ) -> io::Result<()> {
        let stat = fstat(file)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
            || stat.st_uid != geteuid().as_raw()
            || stat.st_mode & 0o002 != 0
            || (stat.st_mode & 0o020 != 0 && (owner_only || shared_group != Some(stat.st_gid)))
        {
            return Err(denied(
                "staging directory is not controlled by the service or its configured shared group",
            ));
        }
        Ok(())
    }

    /// Validate ancestors before resolving OS-owned links such as macOS /tmp.
    /// Unrelated users cannot replace a service-owned child below a sticky /tmp.
    fn trusted_root(path: &Path) -> io::Result<PathBuf> {
        let absolute = match path.is_absolute() {
            true => path.to_path_buf(),
            false => std::env::current_dir()?.join(path),
        };
        let mut prefix = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                    prefix.push(component.as_os_str())
                }
                Component::CurDir => continue,
                Component::ParentDir => {
                    return Err(denied("staging root must not contain parent traversal"));
                }
            }
            let metadata = std::fs::symlink_metadata(&prefix)?;
            if metadata.uid() != 0 && metadata.uid() != geteuid().as_raw() {
                return Err(denied("staging root ancestor is owned by another user"));
            }
            let metadata = match metadata.file_type().is_symlink() {
                true => std::fs::metadata(&prefix)?,
                false => metadata,
            };
            let mode = metadata.permissions().mode();
            let sticky_system_parent =
                metadata.uid() == 0 && mode & 0o1000 != 0 && metadata.is_dir();
            let explicit_shared_root = prefix == absolute && mode & 0o002 == 0;
            if mode & 0o022 != 0 && !sticky_system_parent && !explicit_shared_root {
                return Err(denied(
                    "staging root ancestor is writable by unrelated users",
                ));
            }
        }
        std::fs::canonicalize(absolute)
    }

    fn child_directory(
        parent: &File,
        name: &std::ffi::OsStr,
        shared_group: Option<u32>,
    ) -> io::Result<File> {
        let mode = match shared_group {
            Some(_) => 0o2770,
            None => 0o700,
        };
        match mkdirat(parent, name, Mode::from_raw_mode(mode)) {
            Ok(()) => {}
            Err(error) if error == rustix::io::Errno::EXIST => {}
            Err(error) => return Err(error.into()),
        }
        let directory = File::from(openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        verify_directory(&directory, shared_group, false)?;
        if let Some(group) = shared_group
            && fstat(&directory)?.st_gid != group
        {
            fchown(&directory, None, Some(Gid::from_raw(group)))?;
        }
        fchmod(&directory, Mode::from_raw_mode(mode))?;
        Ok(directory)
    }

    fn fresh_directory(parent: &File, shared_group: Option<u32>) -> io::Result<(String, File)> {
        let name = format!("attempt-{}", aster_forge_utils::id::new_uuid());
        let mode = match shared_group {
            Some(_) => 0o2770,
            None => 0o700,
        };
        mkdirat(parent, name.as_str(), Mode::from_raw_mode(mode))?;
        match child_directory(parent, name.as_ref(), shared_group) {
            Ok(directory) => Ok((name, directory)),
            Err(error) => {
                if let Err(cleanup_error) = unlinkat(parent, name.as_str(), AtFlags::REMOVEDIR) {
                    tracing::warn!(
                        "failed to clean a rejected offline staging directory: {cleanup_error}"
                    );
                }
                Err(error)
            }
        }
    }

    pub(super) fn prepare(task_dir: &Path, shared: bool) -> io::Result<DownloadAttempt> {
        // Forge's lease layout is <temp-root>/tasks/<task-id>/<processing-token>.
        let token = task_dir
            .file_name()
            .ok_or_else(|| denied("missing task lease directory"))?;
        let task_parent = task_dir
            .parent()
            .ok_or_else(|| denied("missing task directory"))?;
        let task = task_parent
            .file_name()
            .ok_or_else(|| denied("missing task identifier"))?;
        let tasks_parent = task_parent
            .parent()
            .ok_or_else(|| denied("missing tasks directory"))?;
        if tasks_parent.file_name() != Some(std::ffi::OsStr::new("tasks")) {
            return Err(denied("offline staging must use the task lease layout"));
        }
        let root_path = trusted_root(
            tasks_parent
                .parent()
                .ok_or_else(|| denied("missing staging root"))?,
        )?;
        let root = open_directory(&root_path)?;
        let root_stat = fstat(&root)?;
        if (root_stat.st_uid != 0 && root_stat.st_uid != geteuid().as_raw())
            || root_stat.st_mode & 0o002 != 0
        {
            return Err(denied(
                "configure an owned non-world-writable offline temporary root",
            ));
        }
        // Group write is an explicit operator-provisioned sharing boundary.
        let chain_group = (root_stat.st_mode & 0o070 == 0o070).then_some(root_stat.st_gid);
        let tasks = child_directory(&root, std::ffi::OsStr::new("tasks"), chain_group)?;
        let task = child_directory(&tasks, task, chain_group)?;
        let parent = child_directory(&task, token, chain_group)?;
        let parent_path = root_path
            .join("tasks")
            .join(
                task_parent
                    .file_name()
                    .ok_or_else(|| denied("missing task identifier"))?,
            )
            .join(token);
        let shared_group = match shared {
            true => chain_group,
            false => None,
        };
        let (name, directory) = fresh_directory(&parent, shared_group)?;
        let path = parent_path.join(name);
        Ok(DownloadAttempt {
            source_path: path.join(SOURCE_NAME),
            parent_path,
            parent,
            directory,
            shared_group,
            guard: TempDirGuard::new(path, "offline engine attempt"),
        })
    }

    fn verify_file(file: &File, shared_group: Option<u32>) -> io::Result<rustix::fs::Stat> {
        let stat = fstat(file)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
            || stat.st_nlink != 1
            || stat.st_mode & 0o002 != 0
            || (stat.st_uid != geteuid().as_raw() && shared_group != Some(stat.st_gid))
        {
            return Err(denied(
                "download output must be a single-linked regular file from the authorized engine",
            ));
        }
        Ok(stat)
    }

    pub(super) fn create_file(path: &Path) -> io::Result<File> {
        if path.file_name() != Some(std::ffi::OsStr::new(SOURCE_NAME)) {
            return Err(denied("offline output must use the source filename"));
        }
        let directory = open_directory(
            path.parent()
                .ok_or_else(|| denied("missing output directory"))?,
        )?;
        verify_directory(&directory, None, true)?;
        let file = File::from(openat(
            &directory,
            SOURCE_NAME,
            OFlags::RDWR
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )?);
        verify_file(&file, None)?;
        Ok(file)
    }

    pub(super) fn seal(attempt: DownloadAttempt) -> io::Result<SealedDownload> {
        let file = File::from(openat(
            &attempt.directory,
            SOURCE_NAME,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let original = verify_file(&file, attempt.shared_group)?;
        let (name, destination) = fresh_directory(&attempt.parent, None)?;
        let directory_path = attempt.parent_path.join(name);
        let guard = TempDirGuard::new(directory_path.clone(), "offline sealed import");
        renameat(&attempt.directory, SOURCE_NAME, &destination, SOURCE_NAME)?;
        let moved = statat(&destination, SOURCE_NAME, AtFlags::SYMLINK_NOFOLLOW)?;
        if moved.st_dev != original.st_dev || moved.st_ino != original.st_ino {
            return Err(denied("download output identity changed during sealing"));
        }
        verify_file(&file, attempt.shared_group)?;
        // Drop removes only this attempt's now-unneeded control files. The
        // returned guard keeps the private import inode through DB publication.
        drop(attempt.guard);
        Ok(SealedDownload {
            path: directory_path.join(SOURCE_NAME),
            file: tokio::fs::File::from_std(file),
            _guard: guard,
        })
    }
}

#[cfg(unix)]
use unix::{create_file, prepare, seal};

#[cfg(not(unix))]
fn prepare(task_dir: &Path, _shared: bool) -> io::Result<DownloadAttempt> {
    let parent_path = std::fs::canonicalize(task_dir)?;
    let path = parent_path.join(format!("attempt-{}", aster_forge_utils::id::new_uuid()));
    std::fs::create_dir(&path)?;
    Ok(DownloadAttempt {
        source_path: path.join(SOURCE_NAME),
        parent_path,
        guard: TempDirGuard::new(path, "offline engine attempt"),
    })
}

#[cfg(not(unix))]
fn create_file(path: &Path) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    options.open(path)
}

#[cfg(not(unix))]
fn seal(attempt: DownloadAttempt) -> io::Result<SealedDownload> {
    if std::fs::symlink_metadata(&attempt.source_path)?
        .file_type()
        .is_symlink()
    {
        return Err(denied("download output must not be a link"));
    }
    let mut open = std::fs::OpenOptions::new();
    open.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Read the reparse point itself so a substituted link is rejected by metadata.
        open.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = open.open(&attempt.source_path)?;
    if !file.metadata()?.is_file() {
        return Err(denied("download output must be regular"));
    }
    let path = attempt
        .parent_path
        .join(format!("import-{}", aster_forge_utils::id::new_uuid()));
    std::fs::create_dir(&path)?;
    let guard = TempDirGuard::new(path.clone(), "offline sealed import");
    std::fs::rename(&attempt.source_path, path.join(SOURCE_NAME))?;
    drop(attempt.guard);
    Ok(SealedDownload {
        path: path.join(SOURCE_NAME),
        file: tokio::fs::File::from_std(file),
        _guard: guard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn fixture() -> (TempDirGuard, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "aster-offline-staging-{}",
            aster_forge_utils::id::new_uuid()
        ));
        std::fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let task = path.join("tasks/42/7");
        (TempDirGuard::new(path, "offline staging test"), task)
    }

    #[tokio::test]
    async fn fallback_attempts_are_unique_and_seal_keeps_the_same_inode_without_copying() {
        let (_root, task) = fixture();
        let first = prepare_attempt(&task, false).await.unwrap();
        let second = prepare_attempt(&task, false).await.unwrap();
        assert_ne!(first.source_path(), second.source_path());
        let source_path = first.source_path().to_path_buf();
        let mut file = create_source(first.source_path()).await.unwrap();
        file.write_all(b"verified bytes").await.unwrap();
        file.flush().await.unwrap();
        let before = file.metadata().await.unwrap();
        drop(file);
        let mut sealed = first.seal().await.unwrap();
        assert!(!source_path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                before.ino(),
                sealed.file_mut().metadata().await.unwrap().ino()
            );
        }
        let mut bytes = Vec::new();
        sealed.file_mut().read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes, b"verified bytes");
        let sealed_path = sealed.path().to_path_buf();
        assert!(sealed_path.exists());
        drop(sealed);
        assert!(!sealed_path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn output_symlinks_hardlinks_and_nonregular_objects_are_rejected_without_truncation() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (root, task) = fixture();
        let victim = root.path().join("private");
        std::fs::write(&victim, b"private bytes").unwrap();
        for kind in ["symlink", "hardlink", "directory", "fifo"] {
            let attempt = prepare_attempt(&task, false).await.unwrap();
            match kind {
                "symlink" => symlink(&victim, attempt.source_path()).unwrap(),
                "hardlink" => std::fs::hard_link(&victim, attempt.source_path()).unwrap(),
                "directory" => std::fs::create_dir(attempt.source_path()).unwrap(),
                _ => assert!(
                    std::process::Command::new("mkfifo")
                        .arg(attempt.source_path())
                        .status()
                        .unwrap()
                        .success()
                ),
            }
            assert!(create_source(attempt.source_path()).await.is_err());
            assert!(attempt.seal().await.is_err());
            assert_eq!(std::fs::read(&victim).unwrap(), b"private bytes");
        }
        let attempt = prepare_attempt(&task, false).await.unwrap();
        let file = create_source(attempt.source_path()).await.unwrap();
        assert_eq!(
            file.metadata().await.unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shared_staging_requires_explicit_group_root_and_never_opens_world_write() {
        use std::os::unix::fs::PermissionsExt;
        let (root, task) = fixture();
        let same_user = prepare_attempt(&task, true).await.unwrap();
        assert_eq!(
            std::fs::metadata(same_user.source_path().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        drop(same_user);
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o2770)).unwrap();
        let shared = prepare_attempt(&task, true).await.unwrap();
        assert_eq!(
            std::fs::metadata(shared.source_path().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o770
        );
        std::fs::write(shared.source_path(), b"shared engine bytes").unwrap();
        let sealed = shared.seal().await.unwrap();
        assert_eq!(
            std::fs::metadata(sealed.path().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::read(sealed.path()).unwrap(),
            b"shared engine bytes"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unsafe_managed_parents_are_rejected_instead_of_repaired() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (root, task) = fixture();
        let tasks = root.path().join("tasks");
        std::fs::create_dir(&tasks).unwrap();
        std::fs::set_permissions(&tasks, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(prepare_attempt(&task, true).await.is_err());
        assert_eq!(
            std::fs::metadata(&tasks).unwrap().permissions().mode() & 0o777,
            0o777
        );
        std::fs::remove_dir(&tasks).unwrap();
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, &tasks).unwrap();
        assert!(prepare_attempt(&task, false).await.is_err());
        assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
    }
}
