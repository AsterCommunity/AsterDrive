use std::io;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use aster_forge_panic::{DEFAULT_CRASH_LOG_PATH, PrivateReportWriter};

pub(crate) fn install() -> Arc<DeferredPrivateReportWriter> {
    let writer = Arc::new(DeferredPrivateReportWriter::new(
        DEFAULT_CRASH_LOG_PATH.into(),
    ));
    aster_forge_panic::install_panic_hook(
        aster_forge_panic::PanicHookConfig::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_REPOSITORY"),
        )
        .with_report_writer(writer.clone()),
    );
    writer
}

pub(crate) struct DeferredPrivateReportWriter {
    path: PathBuf,
    writer: OnceLock<Option<PrivateReportWriter>>,
}

impl DeferredPrivateReportWriter {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            writer: OnceLock::new(),
        }
    }

    pub(crate) fn prepare(&self) {
        let result = (|| {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            PrivateReportWriter::try_new(&self.path)
        })();
        if let Err(error) = &result {
            eprintln!(
                "warning: private crash reports are unavailable at {}: {error}",
                self.path.display()
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
