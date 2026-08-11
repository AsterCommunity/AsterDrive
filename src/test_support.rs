use std::{collections::BTreeSet, path::Path};

pub(crate) mod allocations {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::future::Future;

    thread_local! {
        static MEASURING: Cell<bool> = const { Cell::new(false) };
        static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
        static ALLOCATED_BYTES: Cell<usize> = const { Cell::new(0) };
        static LIVE_BYTES: Cell<usize> = const { Cell::new(0) };
        static PEAK_BYTES: Cell<usize> = const { Cell::new(0) };
    }

    // LIVE_BYTES starts at zero for each measurement window. If an allocation made before the
    // window is freed while measurement is active, saturating subtraction can understate both
    // live and peak bytes. These counters are therefore a safe bounded-growth approximation,
    // not an exact process-memory measurement.

    pub(crate) struct CountingAllocator;

    // SAFETY: every operation delegates to `System` with the original pointer and layout. The
    // thread-local counters are observational and do not alter allocator behavior.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: the caller upholds `GlobalAlloc::alloc`'s layout contract.
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                record_allocation(layout.size());
            }
            pointer
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s layout contract.
            let pointer = unsafe { System.alloc_zeroed(layout) };
            if !pointer.is_null() {
                record_allocation(layout.size());
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            record_deallocation(layout.size());
            // SAFETY: the caller supplies the pointer and layout returned by this allocator.
            unsafe { System.dealloc(pointer, layout) };
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: the caller supplies the original allocation and a valid new size.
            let resized = unsafe { System.realloc(pointer, layout, new_size) };
            if !resized.is_null() {
                record_reallocation(layout.size(), new_size);
            }
            resized
        }
    }

    fn record_allocation(bytes: usize) {
        MEASURING.with(|measuring| {
            if measuring.get() {
                ALLOCATION_COUNT.with(|count| count.set(count.get().saturating_add(1)));
                ALLOCATED_BYTES.with(|total| total.set(total.get().saturating_add(bytes)));
                LIVE_BYTES.with(|live| {
                    let current = live.get().saturating_add(bytes);
                    live.set(current);
                    PEAK_BYTES.with(|peak| peak.set(peak.get().max(current)));
                });
            }
        });
    }

    fn record_deallocation(bytes: usize) {
        MEASURING.with(|measuring| {
            if measuring.get() {
                LIVE_BYTES.with(|live| live.set(live.get().saturating_sub(bytes)));
            }
        });
    }

    fn record_reallocation(previous_bytes: usize, new_bytes: usize) {
        record_deallocation(previous_bytes);
        record_allocation(new_bytes);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct AllocationMeasurement {
        pub(crate) count: usize,
        pub(crate) bytes: usize,
        pub(crate) peak_bytes: usize,
    }

    struct MeasurementGuard;

    impl MeasurementGuard {
        fn start() -> Self {
            MEASURING.with(|measuring| {
                assert!(!measuring.replace(true), "nested allocation measurement");
            });
            ALLOCATION_COUNT.with(|count| count.set(0));
            ALLOCATED_BYTES.with(|bytes| bytes.set(0));
            LIVE_BYTES.with(|bytes| bytes.set(0));
            PEAK_BYTES.with(|bytes| bytes.set(0));
            Self
        }
    }

    impl Drop for MeasurementGuard {
        fn drop(&mut self) {
            MEASURING.with(|measuring| measuring.set(false));
        }
    }

    pub(crate) async fn measure_future<F: Future>(future: F) -> (F::Output, AllocationMeasurement) {
        let guard = MeasurementGuard::start();
        let output = future.await;
        let measurement = AllocationMeasurement {
            count: ALLOCATION_COUNT.with(Cell::get),
            bytes: ALLOCATED_BYTES.with(Cell::get),
            peak_bytes: PEAK_BYTES.with(Cell::get),
        };
        drop(guard);
        (output, measurement)
    }
}

pub(crate) struct FailingAsyncWriter;

impl tokio::io::AsyncWrite for FailingAsyncWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
        _buffer: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(std::io::Error::other("injected write failure")))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

pub(crate) fn snapshot_dir_tree(path: &Path) -> std::io::Result<BTreeSet<String>> {
    fn walk(root: &Path, current: &Path, entries: &mut BTreeSet<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if entry.file_type()?.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = BTreeSet::new();
    if path.exists() {
        walk(path, path, &mut entries)?;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> (PathBuf, aster_forge_utils::raii::TempDirGuard) {
        let path = std::env::temp_dir().join(format!(
            "asterdrive-snapshot-dir-tree-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).expect("snapshot test directory should be created");
        let guard = aster_forge_utils::raii::TempDirGuard::new(
            path.clone(),
            "directory snapshot helper test",
        );
        (path, guard)
    }

    #[test]
    fn missing_directory_has_empty_snapshot() {
        let (root, _guard) = temp_dir("missing");

        assert!(snapshot_dir_tree(&root.join("missing")).unwrap().is_empty());
    }

    #[test]
    fn empty_directory_has_empty_snapshot() {
        let (root, _guard) = temp_dir("empty");

        assert!(snapshot_dir_tree(&root).unwrap().is_empty());
    }

    #[test]
    fn snapshot_normalizes_nested_files_and_marks_directories() {
        let (root, _guard) = temp_dir("nested");
        std::fs::create_dir_all(root.join("nested/empty"))
            .expect("nested directories should be created");
        std::fs::write(root.join("root.txt"), b"root").expect("root file should be written");
        std::fs::write(root.join("nested/file.txt"), b"nested")
            .expect("nested file should be written");

        assert_eq!(
            snapshot_dir_tree(&root).unwrap(),
            BTreeSet::from([
                "nested/".to_string(),
                "nested/empty/".to_string(),
                "nested/file.txt".to_string(),
                "root.txt".to_string(),
            ])
        );
    }

    #[test]
    fn file_root_preserves_read_dir_error() {
        let (root, _guard) = temp_dir("file-root");
        let file = root.join("file.txt");
        std::fs::write(&file, b"file").expect("file root should be written");

        assert!(snapshot_dir_tree(&file).is_err());
    }
}
