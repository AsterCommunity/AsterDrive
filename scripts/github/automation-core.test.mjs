import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyCheckRuns,
  escapeMarkdownCell,
  expectedWorkflows,
  gateConclusion,
  globToRegex,
  incidentFingerprint,
  labelsForFiles,
  parseIncidentState,
  renderDiagnosticsComment,
  renderIncidentBody,
} from "./automation-core.mjs";

test("glob matching handles root and nested recursive paths", () => {
  assert.match("Cargo.toml", globToRegex("**/Cargo.toml"));
  assert.match("crates/model/Cargo.toml", globToRegex("**/Cargo.toml"));
  assert.match("src/webdav/mod.rs", globToRegex("src/webdav/**"));
  assert.doesNotMatch("src/storage/mod.rs", globToRegex("src/webdav/**"));
});

test("file facts produce deterministic language, scope, and risk labels", () => {
  const labels = labelsForFiles([
    "src/webdav/backend/mutation.rs",
    "frontend-panel/src/pages/admin/storage/index.tsx",
    "Cargo.lock",
  ]);
  assert.deepEqual(new Set(labels), new Set([
    "Rust",
    "TypeScript",
    "Dependencies",
    "Scope: Admin UI",
    "Scope: WebDAV",
    "Risk: High",
  ]));
});

test("canonical revision ledger paths retain the versioning scope label", () => {
  for (const path of [
    "src/services/content/version.rs",
    "src/db/repository/revision_repo.rs",
    "crates/aster_drive_model/src/entities/file_revision_history.rs",
    "crates/aster_drive_migration/src/m20260813_000001_canonical_file_revision_ledger.rs",
  ]) {
    assert.ok(labelsForFiles([path]).includes("Scope: Versioning"), path);
  }
});

test("expected workflows mirror path-filtered CI entrypoints", () => {
  assert.deepEqual(new Set(expectedWorkflows(["src/webdav/mod.rs"])), new Set([
    "Rust CI",
    "E2E",
    "Multi-Primary E2E",
    "WebDAV Compatibility",
  ]));
  assert.deepEqual(expectedWorkflows(["README.md"]), []);
  assert.deepEqual(expectedWorkflows(["scripts/github/automation-core.mjs"]), ["Repository Automation"]);
});

test("gate remains pending, fails closed, and succeeds when all required workflows pass", () => {
  const expected = ["Rust CI", "Frontend CI"];
  const pending = classifyCheckRuns([{ workflowName: "Rust CI", status: "completed", conclusion: "success", jobs: [] }], expected);
  assert.deepEqual(gateConclusion(pending), { status: "in_progress", conclusion: null });

  const failed = classifyCheckRuns([
    { workflowName: "Rust CI", status: "completed", conclusion: "failure", jobs: [{ status: "completed", conclusion: "failure" }] },
    { workflowName: "Frontend CI", status: "completed", conclusion: "success", jobs: [] },
  ], expected);
  assert.deepEqual(gateConclusion(failed), { status: "completed", conclusion: "failure" });

  const passed = classifyCheckRuns([
    { workflowName: "Rust CI", status: "completed", conclusion: "success", jobs: [] },
    { workflowName: "Frontend CI", status: "completed", conclusion: "success", jobs: [] },
  ], expected);
  assert.deepEqual(gateConclusion(passed), { status: "completed", conclusion: "success" });

  const workflowLevelFailure = classifyCheckRuns([
    { workflowName: "Rust CI", status: "completed", conclusion: "action_required", jobs: [] },
  ], ["Rust CI"]);
  assert.deepEqual(gateConclusion(workflowLevelFailure), { status: "completed", conclusion: "failure" });
});

test("diagnostic comment is stable and carries actionable failure context", () => {
  const body = renderDiagnosticsComment({
    sha: "1234567890abcdef",
    workflows: [{
      name: "Rust CI",
      state: "failure",
      failedJobs: [{
        name: "OpenAPI and generated SDK drift",
        htmlUrl: "https://example.test/job",
        steps: [{ name: "Verify generated files are up to date", conclusion: "failure" }],
      }],
    }],
  });
  assert.match(body, /asterdrive-ci-diagnostics/);
  assert.match(body, /OpenAPI/);
  assert.match(body, /https:\/\/example\.test\/job/);
  assert.match(body, /正式生成流程/);
});

test("untrusted check names cannot break markdown tables", () => {
  assert.equal(escapeMarkdownCell("job | injected\nrow <tag>"), "job \\| injected row tag");
  const body = renderDiagnosticsComment({
    sha: "1234567890abcdef",
    workflows: [{
      name: "Rust CI",
      state: "failure",
      failedJobs: [{ name: "job | fake\nrow", steps: [{ name: "step | fake", conclusion: "failure" }] }],
    }],
  });
  assert.doesNotMatch(body, /fake\nrow/);
  assert.match(body, /job \\| fake row/);
});

test("incident state and fingerprint are stable across failed job ordering", () => {
  const first = incidentFingerprint({ workflowName: "Rust CI", branch: "master", failedJobs: [{ name: "B" }, { name: "A" }] });
  const second = incidentFingerprint({ workflowName: "Rust CI", branch: "master", failedJobs: [{ name: "A" }, { name: "B" }] });
  assert.equal(first, second);
  const body = renderIncidentBody({
    fingerprint: first,
    workflowName: "Rust CI",
    branch: "master",
    runUrl: "https://example.test/run",
    sha: "abcdef",
    failedJobs: [{ name: "Tests", steps: [] }],
    occurrences: 3,
    recoveryStreak: 1,
  });
  assert.deepEqual(parseIncidentState(body), { occurrences: 3, recoveryStreak: 1 });
});
