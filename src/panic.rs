use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use aster_forge_panic::PrivateReportWriter;

pub(crate) fn install(writer: Arc<DeferredPrivateReportWriter>) {
    aster_forge_panic::install_panic_hook(
        aster_forge_panic::PanicHookConfig::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_REPOSITORY"),
        )
        .with_report_writer(writer.clone()),
    );
}

pub(crate) struct DeferredPrivateReportWriter {
    writer: OnceLock<Option<PrivateReportWriter>>,
}

impl DeferredPrivateReportWriter {
    pub(crate) fn new() -> Self {
        Self {
            writer: OnceLock::new(),
        }
    }

    pub(crate) fn prepare_for_config(&self, config_path: &Path) {
        if self.writer.get().is_some() {
            return;
        }
        let parent = config_path.parent().unwrap_or(Path::new("."));
        let path = parent.join("crash.log");
        self.prepare_at(path);
    }

    fn prepare_at(&self, path: PathBuf) {
        let result = (|| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            PrivateReportWriter::try_new(&path)
        })();
        if let Err(error) = &result {
            eprintln!(
                "warning: private crash reports are unavailable at {}: {error}",
                path.display()
            );
        }
        let _ = self.writer.set(result.ok());
    }
}

impl aster_forge_panic::ReportWriter for DeferredPrivateReportWriter {
    fn write_report(&self, report: &str) -> io::Result<()> {
        self.writer
            .get()
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "private crash report storage is unavailable",
                )
            })
            .and_then(|writer| aster_forge_panic::ReportWriter::write_report(writer, report))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::DeferredPrivateReportWriter;

    #[test]
    fn prepare_creates_the_crash_log_parent_before_hook_installation() {
        let root = aster_forge_test::temp::TestTempDir::new("panic-report-bootstrap");
        let config = root.path().join("nested/config.toml");
        let path = root.path().join("nested/crash.log");
        let writer = DeferredPrivateReportWriter::new();

        writer.prepare_for_config(&config);

        assert!(path.parent().is_some_and(Path::exists));
        assert!(!root.path().join("data").exists());
        #[cfg(target_os = "linux")]
        {
            aster_forge_panic::ReportWriter::write_report(&writer, "panic report")
                .expect("prepared report writer should accept reports");
            assert!(std::fs::metadata(path).is_ok());
        }
    }
}
