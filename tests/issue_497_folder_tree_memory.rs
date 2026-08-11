//! Large-fixture memory benchmark for issue #497 folder delete and restore.

#[macro_use]
#[path = "common/mod.rs"]
mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use actix_web::test;
use aster_drive::db::repository::{file_repo, policy_repo, user_repo};
use aster_drive_model::entities::{file, file_blob};
use aster_forge_file_classification::FileCategory;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};
use serde_json::Value;

struct MeasuringAllocator;

static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicIsize = AtomicIsize::new(0);
static MEASURING: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

// SAFETY: Every allocation operation is delegated to `System` with the original pointer and
// layout. The wrapper only observes sizes through atomics and does not alter allocation results.
unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: This allocator delegates the unchanged layout to the system allocator.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: This allocator delegates the unchanged layout to the system allocator.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(allocation_size(layout.size()), Ordering::Relaxed);
        // SAFETY: The pointer and layout came from the delegated system allocation.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The pointer and layout came from the delegated system allocation.
        let resized = unsafe { System.realloc(pointer, layout, new_size) };
        if !resized.is_null() {
            let old_size = allocation_size(layout.size());
            let new_size_delta = allocation_size(new_size);
            LIVE_BYTES.fetch_add(new_size_delta - old_size, Ordering::Relaxed);
            if MEASURING.load(Ordering::Relaxed) {
                ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
                ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
                update_peak();
            }
        }
        resized
    }
}

fn allocation_size(size: usize) -> isize {
    isize::try_from(size).unwrap_or(isize::MAX)
}

fn record_allocation(bytes: usize) {
    LIVE_BYTES.fetch_add(allocation_size(bytes), Ordering::Relaxed);
    if MEASURING.load(Ordering::Relaxed) {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
        update_peak();
    }
}

fn update_peak() {
    PEAK_BYTES.fetch_max(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

#[derive(Debug)]
struct HeapMeasurement {
    baseline_bytes: u64,
    peak_bytes: u64,
    end_bytes: u64,
    allocated_bytes: u64,
    allocation_count: u64,
}

fn start_heap_measurement() -> u64 {
    let baseline_live = LIVE_BYTES.load(Ordering::SeqCst).max(0);
    let baseline = u64::try_from(baseline_live).expect("non-negative live bytes should fit u64");
    PEAK_BYTES.store(baseline_live, Ordering::SeqCst);
    ALLOCATION_COUNT.store(0, Ordering::SeqCst);
    ALLOCATED_BYTES.store(0, Ordering::SeqCst);
    MEASURING.store(true, Ordering::SeqCst);
    baseline
}

fn finish_heap_measurement(baseline_bytes: u64) -> HeapMeasurement {
    MEASURING.store(false, Ordering::SeqCst);
    HeapMeasurement {
        baseline_bytes,
        peak_bytes: u64::try_from(PEAK_BYTES.load(Ordering::SeqCst).max(0))
            .expect("non-negative peak bytes should fit u64"),
        end_bytes: u64::try_from(LIVE_BYTES.load(Ordering::SeqCst).max(0))
            .expect("non-negative live bytes should fit u64"),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::SeqCst),
        allocation_count: ALLOCATION_COUNT.load(Ordering::SeqCst),
    }
}

fn database_paths(database_path: &Path) -> Vec<PathBuf> {
    let raw = database_path.as_os_str().to_string_lossy();
    ["", "-wal", "-shm", "-journal"]
        .into_iter()
        .map(|suffix| PathBuf::from(format!("{raw}{suffix}")))
        .collect()
}

fn remove_database(paths: &[PathBuf]) {
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to remove {}: {error}", path.display()),
        }
    }
}

fn database_footprint(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .sum()
}

struct DiskMonitor {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    handle: thread::JoinHandle<()>,
}

impl DiskMonitor {
    fn start(paths: Vec<PathBuf>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(database_footprint(&paths)));
        let thread_stop = Arc::clone(&stop);
        let thread_peak = Arc::clone(&peak);
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                thread_peak.fetch_max(database_footprint(&paths), Ordering::Relaxed);
                thread::sleep(Duration::from_millis(2));
            }
            thread_peak.fetch_max(database_footprint(&paths), Ordering::Relaxed);
        });
        Self { stop, peak, handle }
    }

    fn finish(self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.join().expect("disk monitor should join");
        self.peak.load(Ordering::Relaxed)
    }
}

async fn checkpoint(state: &aster_drive::runtime::PrimaryAppState) {
    state
        .writer_db()
        .execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
        .await
        .expect("SQLite checkpoint should succeed");
}

#[expect(
    clippy::too_many_arguments,
    reason = "keeps the emitted metric schema explicit"
)]
fn print_metrics(
    revision: &str,
    operation: &str,
    resource_count: usize,
    file_count: usize,
    status: u16,
    request_elapsed: Duration,
    total_elapsed: Duration,
    heap: HeapMeasurement,
    database_before: u64,
    database_peak: u64,
    database_after: u64,
) {
    println!(
        "ISSUE497_METRICS {}",
        serde_json::json!({
            "revision": revision,
            "operation": operation,
            "resource_count": resource_count,
            "file_count": file_count,
            "status": status,
            "request_us": request_elapsed.as_micros(),
            "total_us": total_elapsed.as_micros(),
            "heap_baseline_bytes": heap.baseline_bytes,
            "heap_peak_bytes": heap.peak_bytes,
            "heap_peak_growth_bytes": heap.peak_bytes.saturating_sub(heap.baseline_bytes),
            "heap_end_bytes": heap.end_bytes,
            "allocated_bytes": heap.allocated_bytes,
            "allocation_count": heap.allocation_count,
            "database_before_bytes": database_before,
            "database_peak_bytes": database_peak,
            "database_peak_growth_bytes": database_peak.saturating_sub(database_before),
            "database_after_bytes": database_after,
        })
    );
}

#[actix_web::test]
#[ignore = "large fixture benchmark; run tests/performance/run-issue-497-folder-tree-memory.sh"]
async fn measure_folder_delete_restore_memory() {
    const INSERT_BATCH: usize = 400;

    let resource_count = std::env::var("ISSUE497_RESOURCES")
        .expect("ISSUE497_RESOURCES must be set")
        .parse::<usize>()
        .expect("ISSUE497_RESOURCES must be an integer");
    assert!(resource_count > 1, "fixture needs a root folder and files");
    let file_count = resource_count - 1;
    let revision = std::env::var("ISSUE497_REVISION").unwrap_or_else(|_| "unknown".to_string());
    let database_path =
        PathBuf::from(std::env::var("ISSUE497_DB_PATH").expect("ISSUE497_DB_PATH must be set"));
    let paths = database_paths(&database_path);
    remove_database(&paths);
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());

    let state = common::setup_with_database_url(&database_url).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let request = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Issue 497 benchmark" }))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 201);
    let body: Value = test::read_body_json(response).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("benchmark user should exist");
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let now = Utc::now();
    let blob = file_blob::ActiveModel {
        hash: Set(format!("issue-497-benchmark-{revision}-{resource_count}")),
        size: Set(0),
        policy_id: Set(policy.id),
        storage_path: Set(format!("issue-497-benchmark-{revision}-{resource_count}")),
        ref_count: Set(i32::try_from(file_count).expect("file count should fit i32")),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("benchmark blob should insert");

    for batch_start in (0..file_count).step_by(INSERT_BATCH) {
        let batch_end = (batch_start + INSERT_BATCH).min(file_count);
        let models = (batch_start..batch_end)
            .map(|index| file::ActiveModel {
                name: Set(format!("benchmark-document-{index:08}-payload.txt")),
                folder_id: Set(Some(folder_id)),
                team_id: Set(None),
                blob_id: Set(blob.id),
                size: Set(0),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                mime_type: Set("text/plain; charset=utf-8".to_string()),
                extension: Set("txt".to_string()),
                compound_extension: Set(None),
                file_category: Set(FileCategory::Document),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            })
            .collect();
        file_repo::create_many(state.writer_db(), models)
            .await
            .expect("benchmark files should insert");
    }

    checkpoint(&state).await;
    let delete_database_before = database_footprint(&paths);
    let delete_disk_monitor = DiskMonitor::start(paths.clone());
    let delete_heap_baseline = start_heap_measurement();
    let delete_total_started = Instant::now();
    let delete_request_started = Instant::now();
    let request = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let response = test::call_service(&app, request).await;
    let delete_request_elapsed = delete_request_started.elapsed();
    let delete_status = response.status().as_u16();
    let delete_failure = if delete_status == 202 {
        None
    } else {
        Some(String::from_utf8_lossy(&test::read_body(response).await).into_owned())
    };
    if delete_status == 202 {
        let stats = aster_drive::services::task::drain(&state)
            .await
            .expect("delete task should drain");
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.succeeded, 1);
    }
    let delete_total_elapsed = delete_total_started.elapsed();
    let delete_heap = finish_heap_measurement(delete_heap_baseline);
    let delete_database_peak = delete_disk_monitor.finish();
    let delete_database_after = database_footprint(&paths);
    print_metrics(
        &revision,
        "delete",
        resource_count,
        file_count,
        delete_status,
        delete_request_elapsed,
        delete_total_elapsed,
        delete_heap,
        delete_database_before,
        delete_database_peak,
        delete_database_after,
    );
    assert!(
        delete_failure.is_none(),
        "delete should queue with 202: {}",
        delete_failure.unwrap_or_default()
    );

    checkpoint(&state).await;
    let restore_database_before = database_footprint(&paths);
    let restore_disk_monitor = DiskMonitor::start(paths.clone());
    let restore_heap_baseline = start_heap_measurement();
    let restore_total_started = Instant::now();
    let restore_request_started = Instant::now();
    let request = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{folder_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let response = test::call_service(&app, request).await;
    let restore_request_elapsed = restore_request_started.elapsed();
    let restore_status = response.status().as_u16();
    let restore_failure = if restore_status == 202 {
        None
    } else {
        Some(String::from_utf8_lossy(&test::read_body(response).await).into_owned())
    };
    if restore_status == 202 {
        let stats = aster_drive::services::task::drain(&state)
            .await
            .expect("restore task should drain");
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.succeeded, 1);
    }
    let restore_total_elapsed = restore_total_started.elapsed();
    let restore_heap = finish_heap_measurement(restore_heap_baseline);
    let restore_database_peak = restore_disk_monitor.finish();
    let restore_database_after = database_footprint(&paths);
    print_metrics(
        &revision,
        "restore",
        resource_count,
        file_count,
        restore_status,
        restore_request_elapsed,
        restore_total_elapsed,
        restore_heap,
        restore_database_before,
        restore_database_peak,
        restore_database_after,
    );
    assert!(
        restore_failure.is_none(),
        "restore should queue with 202: {}",
        restore_failure.unwrap_or_default()
    );
}
