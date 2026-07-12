//! `background_task_repo` 仓储聚合入口。

mod cleanup;
mod common;
mod dispatch;
mod mutation;
mod query;

pub use cleanup::{delete_many, delete_terminal_by_filters, list_expired_terminal};
pub use common::{AdminTaskFilters, TerminalTaskCleanupFilters};
pub use dispatch::{list_claimable, list_claimable_by_kinds, touch_heartbeat, try_claim};
pub use mutation::{
    SystemRuntimeSuccessRefresh, TaskFailureUpdate, TaskProgressUpdate, TaskSuccessUpdate, create,
    mark_failed, mark_progress, mark_retry, mark_succeeded, refresh_system_runtime_success,
    release_processing, reset_for_manual_retry, set_display_name, set_runtime_json,
};
pub use query::{
    count_active_processing_by_kinds, count_pending_or_retry, count_processing, find_by_dedupe_key,
    find_by_id, find_latest_by_kind_and_display_name, find_latest_system_runtime_by_payload,
    find_paginated_all, find_paginated_all_filtered, find_paginated_personal, find_paginated_team,
    list_recent,
};

#[cfg(test)]
mod tests;
