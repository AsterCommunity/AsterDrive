#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
revision="$(git rev-parse --short=12 HEAD)"
resource_counts_csv="${ASTER_ISSUE497_RESOURCE_COUNTS:-100000,500000,1000000}"
temporary_root="${TMPDIR:-/tmp}"
temporary_root="${temporary_root%/}"
if [[ -z "${temporary_root}" ]]; then
  temporary_root="/"
fi
result_directory="${ASTER_ISSUE497_RESULT_DIR:-${temporary_root}/asterdrive-issue-497-${revision}}"
database_directory="${temporary_root}/asterdrive-issue-497-${revision}-databases"

IFS=',' read -r -a resource_counts <<< "${resource_counts_csv}"
mkdir -p "${result_directory}"
mkdir -p "${database_directory}"

node --test \
  "${repository_root}/tests/performance/summarize-issue-497-folder-tree-memory.test.mjs"

for resource_count in "${resource_counts[@]}"; do
  database_path="${database_directory}/${resource_count}.db"
  log_path="${result_directory}/${resource_count}.log"
  printf 'Measuring %s resources at %s\n' "${resource_count}" "${revision}"
  ISSUE497_RESOURCES="${resource_count}" \
  ISSUE497_REVISION="${revision}" \
  ISSUE497_DB_PATH="${database_path}" \
    cargo test \
      --features benchmarks \
      --test issue_497_folder_tree_memory \
      -- \
      --ignored \
      --exact measure_folder_delete_restore_memory \
      --nocapture \
      --test-threads=1 2>&1 | tee "${log_path}"

  if [[ "${ASTER_ISSUE497_KEEP_DATABASES:-0}" != "1" ]]; then
    rm -f \
      "${database_path}" \
      "${database_path}-wal" \
      "${database_path}-shm" \
      "${database_path}-journal"
  fi
done

node \
  "${repository_root}/tests/performance/summarize-issue-497-folder-tree-memory.mjs" \
  "${result_directory}" \
  "${resource_counts[@]}" | tee "${result_directory}/summary.md"

printf 'Results: %s\n' "${result_directory}"
