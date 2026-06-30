use std::path::Path;

use aster_drive::utils::{self, paths};
use aster_drive::webdav::dav::DavPath;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench_file_name_rules(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_name_rules");
    let ascii_name = "quarterly-report-final (12).pdf";
    let unicode_name = "cafe\u{301}-archive-2026-\u{6587}\u{4ef6}.txt";
    let long_name = format!("{}.txt", "a".repeat(251));

    group.bench_function("normalize_validate_ascii", |b| {
        b.iter(|| utils::normalize_validate_name(black_box(ascii_name)))
    });
    group.bench_function("normalize_validate_unicode_nfd", |b| {
        b.iter(|| utils::normalize_validate_name(black_box(unicode_name)))
    });
    group.bench_function("next_copy_name_existing_suffix", |b| {
        b.iter(|| utils::next_copy_name(black_box(ascii_name)))
    });
    group.bench_function("next_copy_name_truncate_long", |b| {
        b.iter(|| utils::next_copy_name(black_box(long_name.as_str())))
    });
    group.finish();
}

fn bench_storage_path_helpers(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_path_helpers");
    let blob_key = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    let base_dir = Path::new("data");
    let config_dir = Path::new("data/config");
    let relative = "../objects/./policy-a/blob.bin";
    let sqlite_url = "sqlite://../db/./asterdrive.db?mode=rwc";

    group.bench_function("storage_path_from_blob_key", |b| {
        b.iter(|| utils::storage_path_from_blob_key(black_box(blob_key)))
    });
    group.bench_function("resolve_config_relative_path", |b| {
        b.iter(|| {
            paths::resolve_config_relative_path(
                black_box(base_dir),
                black_box(config_dir),
                black_box(relative),
            )
        })
    });
    group.bench_function("resolve_config_relative_sqlite_url", |b| {
        b.iter(|| {
            paths::resolve_config_relative_sqlite_url(
                black_box(base_dir),
                black_box(config_dir),
                black_box(sqlite_url),
            )
        })
    });
    group.finish();
}

fn bench_webdav_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("webdav_paths");
    let simple = "/docs/report.txt";
    let encoded = "/teams/42/%E6%96%87%E4%BB%B6/%E6%8A%A5%E5%91%8A.txt";
    let noisy = "/teams/42/a/./b//c/../d/";
    let escaped = "/teams/42/../../secret.txt";

    group.bench_function("dav_path_simple", |b| {
        b.iter(|| DavPath::new(black_box(simple)))
    });
    group.bench_function("dav_path_percent_decoded", |b| {
        b.iter(|| DavPath::new(black_box(encoded)))
    });
    group.bench_function("dav_path_normalized_collection", |b| {
        b.iter(|| DavPath::new(black_box(noisy)))
    });
    group.bench_function("dav_path_escape_rejected", |b| {
        b.iter(|| DavPath::new(black_box(escaped)))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_file_name_rules,
    bench_storage_path_helpers,
    bench_webdav_paths
);
criterion_main!(benches);
