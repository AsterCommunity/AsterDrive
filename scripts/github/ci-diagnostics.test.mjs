import assert from "node:assert/strict";
import test from "node:test";

import { incidentFingerprint, renderIncidentBody } from "./automation-core.mjs";
import { runCiDiagnostics } from "./ci-diagnostics.mjs";

function eventFor(run) {
  return {
    repository: { default_branch: "master" },
    workflow_run: {
      id: 100,
      name: "Rust CI",
      event: "pull_request",
      status: "completed",
      conclusion: "failure",
      head_branch: "feature",
      head_sha: "abc123",
      html_url: "https://example.test/runs/100",
      pull_requests: [{ number: 12 }],
      ...run,
    },
  };
}

function requiredMethods(overrides = {}) {
  return {
    ensureLabel: async () => {},
    listPullsForCommit: async () => [],
    ...overrides,
  };
}

test("failed PR workflow updates the existing gate and creates one diagnostics comment", async () => {
  const checkUpdates = [];
  const comments = [];
  const labelWrites = [];
  let checkListCalls = 0;
  const client = requiredMethods({
    getPull: async () => ({ number: 12, state: "open", head: { sha: "abc123" }, labels: [{ name: "Rust" }, { name: "CI: Running" }] }),
    listPullFiles: async () => [{ filename: "src/lib.rs" }],
    listCheckRuns: async () => {
      checkListCalls += 1;
      if (checkListCalls === 1) {
        return [{
          id: 10,
          name: "Rust CI",
          details_url: "https://github.com/AsterCommunity/AsterDrive/actions/runs/100/job/1",
          check_suite: { app: { slug: "github-actions" } },
        }];
      }
      return [{ id: 99, name: "PR Gate", status: "in_progress", conclusion: null }];
    },
    getWorkflowRun: async () => ({
      id: 100,
      name: "Rust CI",
      head_sha: "abc123",
      status: "completed",
      conclusion: "failure",
      run_started_at: "2026-08-12T00:00:00Z",
      html_url: "https://example.test/runs/100",
    }),
    listWorkflowRunJobs: async () => [{
      name: "Tests and coverage",
      status: "completed",
      conclusion: "failure",
      html_url: "https://example.test/jobs/1",
      steps: [{ name: "Run tests with coverage", conclusion: "failure" }],
    }],
    updateCheckRun: async (id, body) => checkUpdates.push({ id, body }),
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    createCheckRun: async () => assert.fail("existing gate should be updated"),
    listIssueComments: async () => [],
    createIssueComment: async (number, body) => comments.push({ number, body }),
    updateIssueComment: async () => assert.fail("no diagnostics comment exists yet"),
  });

  await runCiDiagnostics({ client, event: eventFor() });
  assert.equal(checkUpdates[0].id, 99);
  assert.equal(checkUpdates[0].body.conclusion, "failure");
  assert.deepEqual(labelWrites, [{ number: 12, labels: ["Rust"] }]);
  assert.equal(comments.length, 1);
  assert.match(comments[0].body, /Tests and coverage/);
});

test("completed gate is superseded instead of reopened when rerun conclusion changes", async () => {
  const created = [];
  const labelWrites = [];
  let checkListCalls = 0;
  const client = requiredMethods({
    getPull: async () => ({ number: 12, state: "open", head: { sha: "abc123" }, labels: [{ name: "Dependencies" }, { name: "CI: Running" }] }),
    listPullFiles: async () => [{ filename: ".cargo/audit.toml" }],
    listCheckRuns: async () => {
      checkListCalls += 1;
      if (checkListCalls === 1) {
        return [{
          id: 10,
          name: "Security Audit",
          details_url: "https://github.com/AsterCommunity/AsterDrive/actions/runs/100/job/1",
          check_suite: { app: { slug: "github-actions" } },
        }];
      }
      return [{ id: 99, name: "PR Gate", status: "completed", conclusion: "failure" }];
    },
    getWorkflowRun: async () => ({
      id: 100,
      name: "Security Audit",
      head_sha: "abc123",
      status: "completed",
      conclusion: "success",
      run_started_at: "2026-08-12T00:00:00Z",
      html_url: "https://example.test/runs/100",
    }),
    listWorkflowRunJobs: async () => [
      { name: "Tests", status: "completed", conclusion: "success", steps: [] },
      { name: "PR Gate", status: "in_progress", conclusion: null, steps: [] },
    ],
    createCheckRun: async (body) => created.push(body),
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    updateCheckRun: async () => assert.fail("completed gate should not be reopened"),
    listIssueComments: async () => [],
    createIssueComment: async () => {},
  });
  await runCiDiagnostics({ client, event: eventFor({ name: "Security Audit", conclusion: "success" }) });
  assert.equal(created.length, 1);
  assert.equal(created[0].conclusion, "success");
  assert.deepEqual(labelWrites, [{ number: 12, labels: ["Dependencies", "CI: Passed"] }]);
});

test("CI-running label remains while another required workflow is pending", async () => {
  const labelWrites = [];
  let checkListCalls = 0;
  const client = requiredMethods({
    getPull: async () => ({ number: 12, state: "open", head: { sha: "abc123" }, labels: [{ name: "Rust" }, { name: "CI: Running" }] }),
    listPullFiles: async () => [{ filename: "Cargo.toml" }],
    listCheckRuns: async () => {
      checkListCalls += 1;
      if (checkListCalls === 1) {
        return [{
          id: 10,
          name: "Rust CI",
          details_url: "https://github.com/AsterCommunity/AsterDrive/actions/runs/100/job/1",
          check_suite: { app: { slug: "github-actions" } },
        }];
      }
      return [{ id: 99, name: "PR Gate", status: "in_progress", conclusion: null }];
    },
    getWorkflowRun: async () => ({
      id: 100,
      name: "Rust CI",
      head_sha: "abc123",
      status: "completed",
      conclusion: "success",
      run_started_at: "2026-08-12T00:00:00Z",
      html_url: "https://example.test/runs/100",
    }),
    listWorkflowRunJobs: async () => [{ name: "Tests", status: "completed", conclusion: "success", steps: [] }],
    updateCheckRun: async () => {},
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    listIssueComments: async () => [],
  });
  await runCiDiagnostics({ client, event: eventFor({ conclusion: "success" }) });
  assert.deepEqual(labelWrites, [{ number: 12, labels: ["Rust", "CI: Running"] }]);
});

test("manual dispatch reconciles a historical workflow run", async () => {
  const checkUpdates = [];
  const labelWrites = [];
  let checkListCalls = 0;
  const sourceRun = eventFor({ name: "Security Audit", conclusion: "success" }).workflow_run;
  const client = requiredMethods({
    getWorkflowRun: async (runId) => {
      assert.equal(runId, 100);
      if (checkListCalls === 0) return sourceRun;
      return {
        ...sourceRun,
        status: "completed",
        conclusion: "success",
        run_started_at: "2026-08-12T00:00:00Z",
      };
    },
    getPull: async () => ({ number: 12, state: "open", head: { sha: "abc123" }, labels: [{ name: "Rust" }, { name: "CI: Running" }] }),
    listPullFiles: async () => [{ filename: ".cargo/audit.toml" }],
    listCheckRuns: async () => {
      checkListCalls += 1;
      if (checkListCalls === 1) {
        return [{
          id: 10,
          name: "Tests",
          details_url: "https://github.com/AsterCommunity/AsterDrive/actions/runs/100/job/1",
          check_suite: { app: { slug: "github-actions" } },
        }];
      }
      return [{ id: 99, name: "PR Gate", status: "in_progress", conclusion: null }];
    },
    listWorkflowRunJobs: async () => [{ name: "Tests", status: "completed", conclusion: "success", steps: [] }],
    updateCheckRun: async (id, body) => checkUpdates.push({ id, body }),
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    listIssueComments: async () => [],
  });
  await runCiDiagnostics({
    client,
    event: { repository: { default_branch: "master" }, inputs: { workflow_run_id: "100" } },
  });
  assert.equal(checkUpdates[0].body.conclusion, "success");
  assert.deepEqual(labelWrites, [{ number: 12, labels: ["Rust", "CI: Passed"] }]);
});

test("default branch failure creates one fingerprinted incident", async () => {
  const issues = [];
  const client = requiredMethods({
    listWorkflowRunJobs: async () => [{
      name: "Tests",
      status: "completed",
      conclusion: "failure",
      html_url: "https://example.test/job",
      steps: [{ name: "Run tests", conclusion: "failure" }],
    }],
    listOpenIssues: async () => [],
    createIssue: async (body) => issues.push(body),
    updateIssue: async () => assert.fail("no incident exists yet"),
  });
  await runCiDiagnostics({
    client,
    event: eventFor({ event: "push", head_branch: "master", pull_requests: [] }),
  });
  assert.equal(issues.length, 1);
  assert.deepEqual(issues[0].labels, ["CI: Failure"]);
  assert.match(issues[0].body, /asterdrive-ci-incident:/);
});

test("workflow-level failure without failed jobs still creates an incident", async () => {
  const issues = [];
  const client = requiredMethods({
    listWorkflowRunJobs: async () => [],
    listOpenIssues: async () => [],
    createIssue: async (body) => issues.push(body),
  });
  await runCiDiagnostics({
    client,
    event: eventFor({ event: "push", head_branch: "master", conclusion: "action_required", pull_requests: [] }),
  });
  assert.equal(issues.length, 1);
  assert.match(issues[0].body, /Workflow-level failure/);
});

test("incident closes only after two consecutive successes", async () => {
  const fingerprint = incidentFingerprint({
    workflowName: "Rust CI",
    branch: "master",
    failedJobs: [{ name: "Tests" }],
  });
  let issue = {
    number: 77,
    body: renderIncidentBody({
      fingerprint,
      workflowName: "Rust CI",
      branch: "master",
      runUrl: "https://example.test/failure",
      sha: "failed",
      failedJobs: [{ name: "Tests", steps: [] }],
      occurrences: 1,
      recoveryStreak: 0,
    }),
  };
  const updates = [];
  const client = requiredMethods({
    listWorkflowRunJobs: async () => [],
    listOpenIssues: async () => [issue],
    updateIssue: async (_number, body) => {
      updates.push(body);
      issue = { ...issue, ...body };
    },
  });
  const success = eventFor({ event: "push", head_branch: "master", conclusion: "success", pull_requests: [] });
  await runCiDiagnostics({ client, event: success });
  assert.match(issue.body, /Recovery streak \| 1 \/ 2/);
  assert.equal(updates.at(-1).state, "open");
  await runCiDiagnostics({ client, event: success });
  assert.equal(updates.at(-1).state, "closed");
  assert.equal(updates.at(-1).state_reason, "completed");
});
