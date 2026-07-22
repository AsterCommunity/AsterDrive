//! Resource-intensive Litmus 0.18 suites.
//!
//! These suites are intentionally separate from the five-suite pull-request
//! baseline. `largefile` transfers about 2 GiB in each direction, while the
//! lockbomb suites execute 20,000 LOCK/UNLOCK iterations per worker.

use super::{LitmusEvaluationMode, LitmusGroup, run_group};
use std::time::Duration;

const LARGEFILE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const LOCKBOMB_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const LOCKBOMB_SINGLE_TIMEOUT: Duration = Duration::from_secs(60 * 60);

pub(super) const TEST_GROUPS: &[LitmusGroup] = &[
    LitmusGroup {
        name: "largefile",
        expected_test_count: 5,
        timeout: LARGEFILE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
    LitmusGroup {
        name: "lockbomb",
        expected_test_count: 3,
        timeout: LOCKBOMB_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
    LitmusGroup {
        name: "lockbomb-single",
        expected_test_count: 3,
        timeout: LOCKBOMB_SINGLE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
];

#[actix_web::test]
#[ignore = "transfers about 2 GiB; run manually with pinned Litmus 0.18"]
async fn test_litmus_largefile() {
    run_group(TEST_GROUPS[0]).await;
}

#[actix_web::test]
#[ignore = "runs 20 threads with 20,000 LOCK/UNLOCK iterations each"]
async fn test_litmus_lockbomb() {
    run_group(TEST_GROUPS[1]).await;
}

#[actix_web::test]
#[ignore = "runs 20,000 LOCK/UNLOCK iterations in one thread"]
async fn test_litmus_lockbomb_single() {
    run_group(TEST_GROUPS[2]).await;
}
