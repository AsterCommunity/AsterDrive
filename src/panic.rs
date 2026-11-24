use std::io;
use std::path::Path;
use std::sync::Arc;

use aster_forge_panic::{PrivateReportWriter, ReportWriter};

pub(crate) fn initialize(config_path: &Path, warn_if_unavailable: bool) {
    let writer = prepare_report_writer(config_path, warn_if_unavailable);
    aster_forge_panic::install_panic_hook(
        aster_forge_panic::PanicHookConfig::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_REPOSITORY"),
        )
        .with_report_writer(writer),
    );
}

fn prepare_report_writer(config_path: &Path, warn_if_unavailable: bool) -> Arc<dyn ReportWriter> {
    let parent = config_path.parent().unwrap_or(Path::new("."));
    let path = parent.join("crash.log");
    match std::fs::create_dir_all(parent).and_then(|()| PrivateReportWriter::try_new(&path)) {
        Ok(writer) => Arc::new(writer),
        Err(error) => {
            if warn_if_unavailable {
                eprintln!(
                    "warning: private crash reports are unavailable at {}: {error}",
                    path.display()
                );
            }
            Arc::new(UnavailableReportWriter)
        }
    }
}

struct UnavailableReportWriter;

impl ReportWriter for UnavailableReportWriter {
    fn write_report(&self, _report: &str) -> io::Result<()> {
        Err(io::Error::other(
            "private crash report storage is unavailable",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::prepare_report_writer;

    #[test]
    fn prepare_creates_the_crash_log_parent_before_hook_installation() {
        let root = aster_forge_test::temp::TestTempDir::new("panic-report-bootstrap");
        let config = root.path().join("nested/config.toml");
        let path = root.path().join("nested/crash.log");
        let writer = prepare_report_writer(&config, true);

        assert!(path.parent().is_some_and(Path::exists));
        assert!(!root.path().join("data").exists());
        #[cfg(target_os = "linux")]
        {
            writer
                .write_report("panic report")
                .expect("prepared report writer should accept reports");
            assert_eq!(std::fs::read_to_string(path).unwrap(), "panic report");
        }
        #[cfg(not(target_os = "linux"))]
        assert!(writer.write_report("panic report").is_err());
    }

    #[test]
    fn unavailable_crash_log_does_not_replace_the_blocking_directory() {
        let root = aster_forge_test::temp::TestTempDir::new("panic-report-unavailable");
        let path = root.path().join("crash.log");
        std::fs::create_dir(&path).unwrap();

        let writer = prepare_report_writer(&root.path().join("config.toml"), false);
        assert!(writer.write_report("panic report").is_err());
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
