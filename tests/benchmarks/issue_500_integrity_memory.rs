//! Opt-in memory benchmark for issue #500 storage-object auditing.

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};

use aster_drive::db::repository::policy_repo;
use aster_drive::services::ops::integrity;

struct MeasuringAllocator;

static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicIsize = AtomicIsize::new(0);
static MEASURING: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;

// SAFETY: Every operation delegates to `System` with the original pointer and layout;
// this wrapper only observes allocation sizes through atomics.
unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Delegate the unchanged layout to the system allocator.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Delegate the unchanged layout to the system allocator.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: The pointer and layout came from the delegated allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The pointer and layout came from the delegated allocator.
        let resized = unsafe { System.realloc(pointer, layout, new_size) };
        if !resized.is_null() {
            LIVE_BYTES.fetch_add(
                new_size as isize - layout.size() as isize,
                Ordering::Relaxed,
            );
            if MEASURING.load(Ordering::Relaxed) {
                ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
                update_peak();
            }
        }
        resized
    }
}

fn record_allocation(size: usize) {
    LIVE_BYTES.fetch_add(size as isize, Ordering::Relaxed);
    if MEASURING.load(Ordering::Relaxed) {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        update_peak();
    }
}

fn update_peak() {
    PEAK_BYTES.fetch_max(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn nonnegative_u64(value: isize) -> u64 {
    u64::try_from(value.max(0)).expect("non-negative allocator counter should fit u64")
}

#[derive(Debug)]
struct Measurement {
    baseline: u64,
    peak: u64,
    end: u64,
    allocations: u64,
}

fn start_measurement() -> u64 {
    let baseline = nonnegative_u64(LIVE_BYTES.load(Ordering::SeqCst));
    PEAK_BYTES.store(
        isize::try_from(baseline).expect("allocator baseline should fit isize"),
        Ordering::SeqCst,
    );
    ALLOCATION_COUNT.store(0, Ordering::SeqCst);
    MEASURING.store(true, Ordering::SeqCst);
    baseline
}

fn finish_measurement(baseline: u64) -> Measurement {
    MEASURING.store(false, Ordering::SeqCst);
    Measurement {
        baseline,
        peak: nonnegative_u64(PEAK_BYTES.load(Ordering::SeqCst)),
        end: nonnegative_u64(LIVE_BYTES.load(Ordering::SeqCst)),
        allocations: ALLOCATION_COUNT.load(Ordering::SeqCst),
    }
}

#[actix_web::test]
#[ignore = "large-fixture benchmark; run explicitly with --ignored"]
async fn measure_integrity_storage_scan_memory() {
    let count = std::env::var("ISSUE500_OBJECT_COUNT")
        .unwrap_or_else(|_| "10000".to_string())
        .parse::<usize>()
        .expect("ISSUE500_OBJECT_COUNT must be an integer");
    assert!(count > 0, "ISSUE500_OBJECT_COUNT must be positive");

    let state = common::setup().await;
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .expect("default policy query should succeed")
        .expect("default policy should exist");
    let driver = state
        .driver_registry
        .get_driver(&policy)
        .expect("default policy driver should build");
    for index in 0..count {
        driver
            .put(&format!("issue-500/{index:08}.bin"), b"fixture")
            .await
            .expect("fixture object should be written");
    }

    let baseline = start_measurement();
    let report = integrity::audit_storage_objects_with_finding_limit(
        state.writer_db(),
        state.driver_registry.as_ref(),
        Some(policy.id),
        aster_drive::config::operations::thumbnail_max_dimension(state.runtime_config()),
        aster_drive::config::operations::image_preview_max_dimension(state.runtime_config()),
        Some(16),
    )
    .await
    .expect("integrity storage scan should succeed");
    let measurement = finish_measurement(baseline);

    assert_eq!(report.untracked_objects_total, count);
    assert_eq!(report.untracked_objects.len(), 16.min(count));
    assert_eq!(report.findings_truncated, count > 16);
    assert!(report.peak_path_batch <= 256);
    eprintln!(
        "issue500 objects={count} baseline={} peak={} end={} allocations={}",
        measurement.baseline, measurement.peak, measurement.end, measurement.allocations
    );
    let base_path = common::local_policy_base_path(&policy);
    drop(driver);
    drop(state);
    std::fs::remove_dir_all(base_path).expect("benchmark fixture directory should be cleaned up");
}
