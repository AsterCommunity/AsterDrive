//! Isolated tracing capture for behavior tests; never installs a global subscriber.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(crate) struct TestLogs(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for TestLogs {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl TestLogs {
    pub(crate) fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + use<> {
        let writer = self.clone();
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .without_time()
            .with_writer(move || writer.clone())
            .finish()
    }

    pub(crate) fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}
