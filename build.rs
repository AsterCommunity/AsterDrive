//! 构建脚本：注入显式构建时间，并为开发/测试构建兜底生成前端占位产物。

use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use std::process::Command;

const BUILD_TIME_ENV: &str = "ASTER_BUILD_TIME";
const BUILD_REVISION_ENV: &str = "ASTER_BUILD_REVISION";
const BUILD_PROFILE_ENV: &str = "ASTER_BUILD_PROFILE";
const BUILD_TARGET_ENV: &str = "ASTER_BUILD_TARGET";
const TEST_SCHEMA_FINGERPRINT_ENV: &str = "ASTER_TEST_SCHEMA_FINGERPRINT";
const FRONTEND_DIST_ENV: &str = "ASTER_FRONTEND_DIST_DIR";
const FALLBACK_MARKER_FILE: &str = ".asterdrive-frontend-fallback";
const FALLBACK_MARKER_CONTENT: &str = "asterdrive-frontend-fallback-v1\n";
const LEGACY_FALLBACK_TEXT: &str = "Frontend Not Built";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=frontend-panel/dist");
    println!("cargo:rerun-if-env-changed={BUILD_TIME_ENV}");
    println!("cargo:rerun-if-env-changed={BUILD_REVISION_ENV}");

    configure_build_time()?;

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|error| io::Error::other(format!("missing CARGO_MANIFEST_DIR: {error}")))?;
    configure_build_revision(Path::new(&manifest_dir))?;
    configure_build_features();
    println!("cargo:rerun-if-changed=src");
    configure_test_schema_fingerprint(Path::new(&manifest_dir))?;
    let dist_path = Path::new(&manifest_dir).join("frontend-panel/dist");
    let out_dir = env::var("OUT_DIR")
        .map_err(|error| io::Error::other(format!("missing OUT_DIR: {error}")))?;
    let fallback_dist_path = Path::new(&out_dir).join("frontend-dist-fallback");
    let profile = env::var("PROFILE")
        .map_err(|error| io::Error::other(format!("missing PROFILE: {error}")))?;
    let target =
        env::var("TARGET").map_err(|error| io::Error::other(format!("missing TARGET: {error}")))?;
    configure_build_context(&profile, &target)?;

    let selected_dist_path = match frontend_dist_state(&dist_path)? {
        FrontendDistState::Real => dist_path,
        FrontendDistState::Missing if fallback_allowed(&profile) => {
            eprintln!(
                "Warning: frontend-panel/dist is missing; generating isolated development fallback assets"
            );
            create_fallback_files(&fallback_dist_path)?;
            fallback_dist_path
        }
        FrontendDistState::Fallback if fallback_allowed(&profile) => {
            eprintln!(
                "Warning: frontend-panel/dist contains fallback assets; generating a clean isolated fallback"
            );
            create_fallback_files(&fallback_dist_path)?;
            fallback_dist_path
        }
        FrontendDistState::Missing | FrontendDistState::Fallback => {
            return Err(io::Error::other(format!(
                "frontend-panel/dist does not contain a production frontend for the {profile} profile; run `cd frontend-panel && bun install --frozen-lockfile && bun run build` before building"
            ))
            .into());
        }
    };

    let selected_dist_path = selected_dist_path.to_str().ok_or_else(|| {
        io::Error::other("selected frontend dist path must contain valid Unicode")
    })?;
    println!("cargo:rustc-env={FRONTEND_DIST_ENV}={selected_dist_path}");

    Ok(())
}

fn configure_test_schema_fingerprint(manifest_dir: &Path) -> io::Result<()> {
    let migration_source = manifest_dir.join("crates/aster_drive_migration/src");
    println!("cargo:rerun-if-changed={}", migration_source.display());

    let mut source_files = Vec::new();
    collect_rust_source_files(&migration_source, &mut source_files)?;
    source_files.sort();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for source_file in source_files {
        source_file
            .strip_prefix(manifest_dir)
            .unwrap_or(&source_file)
            .hash(&mut hasher);
        fs::read(&source_file)?.hash(&mut hasher);
    }
    println!(
        "cargo:rustc-env={TEST_SCHEMA_FINGERPRINT_ENV}=migration-src-{:016x}",
        hasher.finish()
    );
    Ok(())
}

fn collect_rust_source_files(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrontendDistState {
    Missing,
    Fallback,
    Real,
}

fn configure_build_time() -> io::Result<()> {
    let value = match env::var(BUILD_TIME_ENV) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => {
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
        }
        Err(env::VarError::NotUnicode(_)) => {
            return Err(io::Error::other(format!(
                "{BUILD_TIME_ENV} must contain valid Unicode"
            )));
        }
    };

    let value = value.trim();
    if value.is_empty() {
        return Err(io::Error::other(format!(
            "{BUILD_TIME_ENV} must not be empty when set"
        )));
    }
    if value.contains('\r') || value.contains('\n') {
        return Err(io::Error::other(format!(
            "{BUILD_TIME_ENV} must be a single-line value"
        )));
    }
    println!("cargo:rustc-env={BUILD_TIME_ENV}={value}");

    Ok(())
}

fn configure_build_revision(manifest_dir: &Path) -> io::Result<()> {
    let revision_override =
        env::var(BUILD_REVISION_ENV)
            .map(Some)
            .or_else(|error| match error {
                env::VarError::NotPresent => Ok(None),
                _ => Err(io::Error::other(
                    "ASTER_BUILD_REVISION must contain valid Unicode",
                )),
            })?;
    let identity = source_identity(manifest_dir, revision_override.as_deref())?;
    println!("cargo:rustc-env={BUILD_REVISION_ENV}={}", identity.revision);
    println!(
        "cargo:rustc-env=ASTER_BUILD_DIRTY={}",
        identity
            .dirty
            .map_or("unknown", |dirty| if dirty { "true" } else { "false" })
    );
    for path in identity.watch_paths {
        if let Some(path) = nearest_existing_watch_path(&path) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    Ok(())
}

fn nearest_existing_watch_path(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| candidate.exists())
        .map(Path::to_path_buf)
}

fn configure_build_features() {
    let mut features: Vec<String> = env::vars_os()
        .filter_map(|(key, _)| {
            let key = key.to_str()?;
            let feature = key.strip_prefix("CARGO_FEATURE_")?;
            if matches!(feature, "DEFAULT" | "FULL") {
                return None;
            }
            Some(feature.to_ascii_lowercase().replace('_', "-"))
        })
        .collect();
    features.sort();
    println!(
        "cargo:rustc-env=ASTER_BUILD_FEATURES={}",
        features.join(",")
    );
}

fn configure_build_context(profile: &str, target: &str) -> io::Result<()> {
    for (name, value) in [(BUILD_PROFILE_ENV, profile), (BUILD_TARGET_ENV, target)] {
        if value.is_empty() || value.contains(['\r', '\n']) {
            return Err(io::Error::other(format!(
                "{name} must be a non-empty single-line value"
            )));
        }
        println!("cargo:rustc-env={name}={value}");
    }

    Ok(())
}

fn fallback_allowed(profile: &str) -> bool {
    matches!(profile, "debug" | "test")
}

fn frontend_dist_state(dist_path: &Path) -> io::Result<FrontendDistState> {
    if !dist_path.exists() {
        return Ok(FrontendDistState::Missing);
    }

    if dist_path.join(FALLBACK_MARKER_FILE).exists() {
        return Ok(FrontendDistState::Fallback);
    }

    let index_path = dist_path.join("index.html");
    if !index_path.exists() {
        return Ok(FrontendDistState::Missing);
    }

    let index_html = fs::read_to_string(index_path)?;
    if index_html.contains(LEGACY_FALLBACK_TEXT) {
        return Ok(FrontendDistState::Fallback);
    }

    Ok(FrontendDistState::Real)
}

fn create_fallback_files(dist_path: &Path) -> io::Result<()> {
    if dist_path.exists() {
        fs::remove_dir_all(dist_path)?;
    }
    fs::create_dir_all(dist_path)?;
    fs::write(
        dist_path.join(FALLBACK_MARKER_FILE),
        FALLBACK_MARKER_CONTENT,
    )?;

    let fallback_html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <link rel="icon" type="image/svg+xml" href="%ASTERDRIVE_FAVICON_URL%" />
    <link rel="apple-touch-icon" href="%ASTERDRIVE_FAVICON_URL%" />
    <link rel="preload" as="image" href="%ASTERDRIVE_WORDMARK_LIGHT_URL%" media="(min-width: 1024px), (prefers-color-scheme: dark)" />
    <link rel="preload" as="image" href="%ASTERDRIVE_WORDMARK_DARK_URL%" media="(max-width: 1023px) and (prefers-color-scheme: light)" />
    <meta name="description" content="%ASTERDRIVE_DESCRIPTION%" />
    <meta http-equiv="Content-Security-Policy" content="%ASTERDRIVE_CSP%" />
    <meta name="asterdrive-version" content="%ASTERDRIVE_VERSION%" />
    <title>%ASTERDRIVE_TITLE%</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            max-width: 600px;
            margin: 100px auto;
            padding: 20px;
            text-align: center;
            color: #333;
        }
        .warning {
            background: #fff3cd;
            border: 1px solid #ffeaa7;
            padding: 20px;
            border-radius: 8px;
            margin: 20px 0;
        }
        code {
            background: #f1f3f4;
            padding: 2px 6px;
            border-radius: 4px;
            font-family: monospace;
        }
    </style>
</head>
<body>
    <h1>%ASTERDRIVE_TITLE%</h1>
    <div class="warning">
        <h2>Frontend Not Built</h2>
        <p>The admin panel needs to be built before it can be served.</p>
        <p>Run:</p>
        <p><code>cd frontend-panel && bun install --frozen-lockfile && bun run build</code></p>
    </div>
    <p>API is still available at <code>/api/v1/</code></p>
</body>
</html>"#;

    fs::write(dist_path.join("index.html"), fallback_html)?;

    fs::write(dist_path.join("favicon.ico"), [])?;

    fs::create_dir_all(dist_path.join("assets"))?;
    fs::write(
        dist_path.join("assets").join("fallback.css"),
        "body{background:#f8fafc;}\n",
    )?;
    fs::write(
        dist_path.join("sw.js"),
        "self.addEventListener('install',()=>self.skipWaiting());self.addEventListener('activate',event=>event.waitUntil(self.clients.claim()));\n",
    )?;
    fs::write(
        dist_path.join("manifest.webmanifest"),
        r##"{"name":"AsterDrive","short_name":"AsterDrive","start_url":"/","display":"standalone","background_color":"#ffffff","theme_color":"#0f172a","icons":[]}"##,
    )?;
    Ok(())
}

#[derive(Debug)]
struct SourceIdentity {
    revision: String,
    dirty: Option<bool>,
    watch_paths: Vec<PathBuf>,
}

fn source_identity(
    manifest_dir: &Path,
    revision_override: Option<&str>,
) -> io::Result<SourceIdentity> {
    let override_value = revision_override.map(str::trim);
    if let Some(value) = override_value
        && value != "unknown"
        && (!(7..=64).contains(&value.len()) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(io::Error::other(
            "ASTER_BUILD_REVISION must be 'unknown' or a 7-64 character hexadecimal Git revision",
        ));
    }
    let root = git_output(manifest_dir, &["rev-parse", "--show-toplevel"])
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| PathBuf::from(value.trim()));
    let own_repository = root.as_deref().and_then(|path| path.canonicalize().ok())
        == manifest_dir.canonicalize().ok()
        && root.is_some();
    let head = own_repository
        .then(|| git_output(manifest_dir, &["rev-parse", "--verify", "HEAD"]))
        .flatten()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|value| value.trim().to_owned());
    let revision = override_value
        .map(str::to_owned)
        .or_else(|| head.clone())
        .unwrap_or_else(|| "unknown".into());
    let dirty = head
        .as_deref()
        .filter(|head| {
            head.get(..revision.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&revision))
        })
        .and_then(|_| {
            git_output(
                manifest_dir,
                &["status", "--porcelain", "--untracked-files=normal"],
            )
        })
        .map(|status| !status.is_empty());
    let mut watch_paths = Vec::new();
    if own_repository {
        for git_path in ["HEAD", "index", "packed-refs"] {
            if let Some(path) = git_output(manifest_dir, &["rev-parse", "--git-path", git_path])
                .and_then(|bytes| String::from_utf8(bytes).ok())
            {
                let path = PathBuf::from(path.trim());
                watch_paths.push(if path.is_absolute() {
                    path
                } else {
                    manifest_dir.join(path)
                });
            }
        }
        if let Some(reference) = git_output(manifest_dir, &["symbolic-ref", "--quiet", "HEAD"])
            .and_then(|bytes| String::from_utf8(bytes).ok())
            && let Some(path) =
                git_output(manifest_dir, &["rev-parse", "--git-path", reference.trim()])
                    .and_then(|bytes| String::from_utf8(bytes).ok())
        {
            let path = PathBuf::from(path.trim());
            watch_paths.push(if path.is_absolute() {
                path
            } else {
                manifest_dir.join(path)
            });
        }
        if let Some(files) = git_output(
            manifest_dir,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
        ) {
            for file in files
                .split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
            {
                if let Ok(file) = std::str::from_utf8(file)
                    && !file.contains(['\r', '\n'])
                {
                    watch_paths.push(manifest_dir.join(file));
                }
            }
        }
    }
    Ok(SourceIdentity {
        revision,
        dirty,
        watch_paths,
    })
}

fn git_output(directory: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_forge_utils::raii::TempDirGuard;
    fn directory(label: &str) -> TempDirGuard {
        let guard = TempDirGuard::new(
            std::env::temp_dir().join(format!("aster-build-{label}-{}", uuid::Uuid::new_v4())),
            "build identity fixture",
        );
        std::fs::create_dir_all(guard.path()).unwrap();
        guard
    }

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=Build Identity Test",
                "-c",
                "user.email=build-test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=empty-hooks",
                "-c",
                "core.fsmonitor=false",
            ])
            .args(args)
            .current_dir(directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn repository(path: &Path) -> String {
        git(path, &["init", "--quiet"]);
        std::fs::write(path.join("source.rs"), "original\n").unwrap();
        git(path, &["add", "source.rs"]);
        git(path, &["commit", "--quiet", "-m", "fixture"]);
        git(path, &["rev-parse", "HEAD"])
    }

    #[test]
    fn revision_and_dirty_follow_clean_unstaged_staged_and_untracked_source() {
        let root = directory("repository");
        let head = repository(root.path());
        let clean = source_identity(root.path(), None).unwrap();
        assert_eq!(clean.revision, head);
        assert_eq!(clean.dirty, Some(false));
        assert!(clean.watch_paths.iter().any(|path| path.ends_with("HEAD")));
        assert!(clean.watch_paths.iter().any(|path| path.ends_with("index")));
        assert!(clean.watch_paths.contains(&root.path().join("source.rs")));

        std::fs::write(root.path().join("source.rs"), "modified\n").unwrap();
        assert_eq!(
            source_identity(root.path(), None).unwrap().dirty,
            Some(true)
        );
        git(root.path(), &["add", "source.rs"]);
        assert_eq!(
            source_identity(root.path(), None).unwrap().dirty,
            Some(true)
        );
        git(root.path(), &["commit", "--quiet", "-m", "update"]);
        let updated = source_identity(root.path(), None).unwrap();
        assert_ne!(updated.revision, head);
        assert_eq!(updated.dirty, Some(false));

        std::fs::write(root.path().join("new.rs"), "untracked\n").unwrap();
        let untracked = source_identity(root.path(), None).unwrap();
        assert_eq!(untracked.dirty, Some(true));
        assert!(untracked.watch_paths.contains(&root.path().join("new.rs")));
    }

    #[test]
    fn missing_git_and_source_paths_fall_back_to_existing_parents() {
        let root = directory("missing-watch-paths");
        let head = repository(root.path());
        let packed_refs = root.path().join(".git/packed-refs");
        if packed_refs.exists() {
            std::fs::remove_file(&packed_refs).unwrap();
        }
        let packed_refs_identity = source_identity(root.path(), None).unwrap();
        assert!(packed_refs_identity.watch_paths.contains(&packed_refs));
        assert_eq!(
            nearest_existing_watch_path(&packed_refs),
            packed_refs.parent().map(Path::to_path_buf)
        );

        let branch = git(root.path(), &["symbolic-ref", "--short", "HEAD"]);
        let loose_ref = root.path().join(git(
            root.path(),
            &["rev-parse", "--git-path", &format!("refs/heads/{branch}")],
        ));
        std::fs::remove_file(&loose_ref).unwrap();
        let deleted_ref_identity = source_identity(root.path(), None).unwrap();
        assert_eq!(deleted_ref_identity.revision, "unknown");
        assert!(deleted_ref_identity.watch_paths.contains(&loose_ref));
        assert_eq!(
            nearest_existing_watch_path(&loose_ref),
            loose_ref.parent().map(Path::to_path_buf)
        );

        let root = directory("deleted-tracked-watch");
        let _head = repository(root.path());
        let tracked = root.path().join("source.rs");
        std::fs::remove_file(&tracked).unwrap();
        let deleted_file_identity = source_identity(root.path(), None).unwrap();
        assert!(deleted_file_identity.watch_paths.contains(&tracked));
        assert_eq!(
            nearest_existing_watch_path(&tracked),
            tracked.parent().map(Path::to_path_buf)
        );

        // Keep the fixture's revision in scope so this test also proves the
        // repository remained readable before the deliberate ref deletion.
        assert!(!head.is_empty());
    }

    #[test]
    fn revision_override_retains_unknown_provenance_in_archives_and_unrelated_revisions() {
        let root = directory("archive");
        let head = repository(root.path());
        let short = &head[..12];
        assert_eq!(
            source_identity(root.path(), Some(short)).unwrap().dirty,
            Some(false)
        );
        assert_eq!(
            source_identity(root.path(), Some("abcdef012345"))
                .unwrap()
                .dirty,
            None
        );
        assert_eq!(
            source_identity(root.path(), Some("unknown")).unwrap().dirty,
            None
        );
        for invalid in [
            "",
            " ",
            "bad",
            "not-a-git-revision",
            "abcdef0\ncredential-marker",
            &"a".repeat(65),
        ] {
            let error = source_identity(root.path(), Some(invalid))
                .unwrap_err()
                .to_string();
            assert!(!error.contains("credential-marker"));
        }

        // A source archive nested inside another checkout must not inherit that checkout's revision.
        let archive = root.path().join("archive");
        std::fs::create_dir_all(&archive).unwrap();
        let unknown = source_identity(&archive, None).unwrap();
        assert_eq!(unknown.revision, "unknown");
        assert_eq!(unknown.dirty, None);
        assert!(unknown.watch_paths.is_empty());
        let explicit = source_identity(&archive, Some(short)).unwrap();
        assert_eq!(explicit.revision, short);
        assert_eq!(explicit.dirty, None);
    }

    #[test]
    fn linked_worktree_uses_its_own_head_and_index() {
        let root = directory("worktree");
        let repository_path = root.path().join("repository");
        std::fs::create_dir_all(&repository_path).unwrap();
        let head = repository(&repository_path);
        let checkout = root.path().join("linked");
        git(
            &repository_path,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                checkout.to_str().unwrap(),
                "HEAD",
            ],
        );
        assert!(checkout.join(".git").is_file());
        let clean = source_identity(&checkout, None).unwrap();
        assert_eq!(clean.revision, head);
        assert_eq!(clean.dirty, Some(false));
        let index = PathBuf::from(git(&checkout, &["rev-parse", "--git-path", "index"]));
        assert!(clean.watch_paths.contains(&index));
        std::fs::write(checkout.join("source.rs"), "linked changes\n").unwrap();
        assert_eq!(source_identity(&checkout, None).unwrap().dirty, Some(true));
        assert_eq!(
            source_identity(&repository_path, None).unwrap().dirty,
            Some(false)
        );
    }
}
