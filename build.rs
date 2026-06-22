//! 构建脚本：注入构建时间并兜底生成前端占位产物。

use std::env;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=frontend-panel/dist");

    // 构建时间
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    println!("cargo:rustc-env=ASTER_BUILD_TIME={now}");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|error| io::Error::other(format!("missing CARGO_MANIFEST_DIR: {error}")))?;
    let dist_path = Path::new(&manifest_dir).join("frontend-panel/dist");

    if !dist_path.exists() {
        eprintln!("Warning: frontend-panel/dist directory not found!");
        eprintln!("Please build the frontend first:");
        eprintln!("  cd frontend-panel && bun install && bun run build");

        create_fallback_files(&dist_path)?;
    }

    Ok(())
}

fn create_fallback_files(dist_path: &Path) -> io::Result<()> {
    fs::create_dir_all(dist_path)?;

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
    <title>%ASTERDRIVE_TITLE% - Frontend Not Built</title>
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
        <p><code>cd frontend-panel && bun install && bun run build</code></p>
    </div>
    <p>API is still available at <code>/api/v1/</code></p>
</body>
</html>"#;

    fs::write(dist_path.join("index.html"), fallback_html)?;

    fs::write(dist_path.join("favicon.ico"), [])?;

    fs::create_dir_all(dist_path.join("assets"))?;
    Ok(())
}
