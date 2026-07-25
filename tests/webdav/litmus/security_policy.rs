//! Implementation-specific Litmus 0.18 security-policy suites.
//!
//! `protected` exercises a server-owned metadata namespace rather than an
//! RFC 4918 conformance requirement. The pinned input is explicit so its
//! result cannot silently change with the caller's environment.
//!
//! AsterDrive stores dead properties in `entity_properties`, keyed by resolved
//! file/folder entities, and intentionally reserves no WebDAV directory name
//! for that storage. The observed policy differences show that `.DAV` is an
//! ordinary user path; by themselves they do not indicate exposure of the
//! internal property records.

use super::{LitmusEvaluationMode, LitmusGroup, run_group};
use std::time::Duration;

const SECURITY_POLICY_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const PROTECTED_ENVIRONMENT: &[(&str, &str)] = &[("TEST_PROTECTED", ".DAV")];

pub(super) const TEST_GROUPS: &[LitmusGroup] = &[LitmusGroup {
    name: "protected",
    expected_test_count: 25,
    timeout: SECURITY_POLICY_TIMEOUT,
    environment: PROTECTED_ENVIRONMENT,
    evaluation_mode: LitmusEvaluationMode::Probe,
}];

#[actix_web::test]
#[ignore = "implementation-specific probe; AsterDrive reserves no WebDAV property directory"]
async fn test_litmus_protected() {
    run_group(TEST_GROUPS[0]).await;
}
