#!/usr/bin/env bash
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
revision="$(git rev-parse --short=12 HEAD)"
resource_counts_csv="${ASTER_ISSUE497_RESOURCE_COUNTS:-100000,500000,1000000}"
scenarios_csv="${ASTER_ISSUE497_SCENARIOS:-wide_files,wide_folders,deep_chain}"
temporary_root="${TMPDIR:-/tmp}"
temporary_root="${temporary_root%/}"
if [[ -z "${temporary_root}" ]]; then
  temporary_root="/"
fi
result_directory="${ASTER_ISSUE497_RESULT_DIR:-${temporary_root}/asterdrive-issue-497-${revision}}"
database_directory="${temporary_root}/asterdrive-issue-497-${revision}-databases"

IFS=',' read -r -a resource_counts <<< "${resource_counts_csv}"
IFS=',' read -r -a scenarios <<< "${scenarios_csv}"
mkdir -p "${result_directory}"
mkdir -p "${database_directory}"

node --test \
  "${repository_root}/tests/performance/summarize-issue-497-folder-tree-memory.test.mjs"

scenario_specs=()

run_scenario() {
  local scenario="$1"
  local resource_count="$2"
  local database_path="${database_directory}/${scenario}-${resource_count}.db"
  local log_path="${result_directory}/${scenario}-${resource_count}.log"
  scenario_specs+=("${scenario}:${resource_count}")

  printf 'Measuring %s with %s resources at %s\n' \
    "${scenario}" "${resource_count}" "${revision}"
  ISSUE497_SCENARIO="${scenario}" \
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
}

for scenario in "${scenarios[@]}"; do
  case "${scenario}" in
    wide_files)
      for resource_count in "${resource_counts[@]}"; do
        run_scenario "${scenario}" "${resource_count}"
      done
      ;;
    wide_folders)
      # Root plus 2,001 direct children crosses the 2,000-entry frontier limit.
      run_scenario "${scenario}" "2002"
      ;;
    deep_chain)
      # Root plus 129 descendants crosses the maximum accepted depth of 128.
      run_scenario "${scenario}" "130"
      ;;
    *)
      printf 'Unknown ASTER_ISSUE497_SCENARIOS value: %s\n' "${scenario}" >&2
      exit 1
      ;;
  esac
done

node \
  "${repository_root}/tests/performance/summarize-issue-497-folder-tree-memory.mjs" \
  "${result_directory}" \
  "${scenario_specs[@]}" | tee "${result_directory}/summary.md"

printf 'Results: %s\n' "${result_directory}"
