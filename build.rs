//! 构建脚本：注入显式构建时间，并为开发/测试构建兜底生成前端占位产物。

use std::env;
use std::fs;
use std::io;
use std::path::Path;

const BUILD_TIME_ENV: &str = "ASTER_BUILD_TIME";
const FRONTEND_DIST_ENV: &str = "ASTER_FRONTEND_DIST_DIR";
const FALLBACK_MARKER_FILE: &str = ".asterdrive-frontend-fallback";
const FALLBACK_MARKER_CONTENT: &str = "asterdrive-frontend-fallback-v1\n";
const LEGACY_FALLBACK_TEXT: &str = "Frontend Not Built";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=frontend-panel/dist");
    println!("cargo:rerun-if-env-changed={BUILD_TIME_ENV}");

    configure_build_time()?;

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|error| io::Error::other(format!("missing CARGO_MANIFEST_DIR: {error}")))?;
    let dist_path = Path::new(&manifest_dir).join("frontend-panel/dist");
    let out_dir = env::var("OUT_DIR")
        .map_err(|error| io::Error::other(format!("missing OUT_DIR: {error}")))?;
    let fallback_dist_path = Path::new(&out_dir).join("frontend-dist-fallback");
    let profile = env::var("PROFILE")
        .map_err(|error| io::Error::other(format!("missing PROFILE: {error}")))?;

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
