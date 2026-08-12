import assert from "node:assert/strict";
import test from "node:test";

import { runPrAutomation } from "./pr-automation.mjs";

function baseClient(overrides = {}) {
  return {
    owner: "AsterCommunity",
    repo: "AsterDrive",
    ensureLabel: async () => {},
    getPull: async (number) => pull({ number }),
    listPullFiles: async () => [],
    listCheckRuns: async () => [],
    createCheckRun: async () => {},
    listIssueComments: async () => [],
    updateIssueComment: async () => {},
    setIssueLabels: async () => {},
    graphql: async () => ({
      data: { repository: { pullRequest: { closingIssuesReferences: { nodes: [] } } } },
    }),
    ...overrides,
  };
}

function pull(overrides = {}) {
  return {
    number: 12,
    merged: false,
    labels: [],
    head: { sha: "abc123" },
    ...overrides,
  };
}

test("open PR preserves manual labels, replaces managed labels, and initializes a pending gate", async () => {
  const labelWrites = [];
  const gates = [];
  const client = baseClient({
    getPull: async () => pull({ labels: [{ name: "Priority: High" }, { name: "Scope: Storage" }, { name: "CI: Passed" }] }),
    listPullFiles: async () => [{ filename: "src/webdav/mod.rs" }],
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    createCheckRun: async (body) => gates.push(body),
  });
  await runPrAutomation({
    client,
    event: { action: "opened", pull_request: pull() },
  });
  assert.deepEqual(new Set(labelWrites[0].labels), new Set(["Priority: High", "Rust", "Scope: WebDAV", "Risk: High", "CI: Running"]));
  assert.equal(gates[0].status, "in_progress");
  assert.match(gates[0].output.summary, /WebDAV Compatibility: pending/);
});

test("metadata-only PR initializes a successful gate", async () => {
  const gates = [];
  const labelWrites = [];
  const client = baseClient({
    listPullFiles: async () => [{ filename: "README.md" }],
    createCheckRun: async (body) => gates.push(body),
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
  });
  await runPrAutomation({ client, event: { action: "opened", pull_request: pull() } });
  assert.equal(gates[0].status, "completed");
  assert.equal(gates[0].conclusion, "success");
  assert.equal(labelWrites.length, 1);
  assert.ok(!labelWrites[0].labels.includes("CI: Running"));
});

test("metadata-only update resolves an existing stale diagnostics comment", async () => {
  const updates = [];
  const client = baseClient({
    listPullFiles: async () => [{ filename: "README.md" }],
    listIssueComments: async () => [{ id: 9, body: "<!-- asterdrive-ci-diagnostics -->\nold failure" }],
    updateIssueComment: async (id, body) => updates.push({ id, body }),
  });
  await runPrAutomation({ client, event: { action: "synchronize", pull_request: pull() } });
  assert.equal(updates[0].id, 9);
  assert.match(updates[0].body, /No path-filtered CI workflows are required/);
});

test("reopened PR preserves passed CI state from its current gate", async () => {
  const labelWrites = [];
  const client = baseClient({
    getPull: async () => pull({ labels: [{ name: "Rust" }, { name: "CI: Running" }] }),
    listPullFiles: async () => [{ filename: "Cargo.toml" }],
    listCheckRuns: async () => [{ id: 99, name: "PR Gate", status: "completed", conclusion: "success" }],
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
  });
  await runPrAutomation({ client, event: { action: "reopened", pull_request: pull() } });
  assert.deepEqual(new Set(labelWrites[0].labels), new Set(["Rust", "Dependencies", "CI: Passed"]));
});

test("closed PR keeps linked issue in progress while another closing PR is open", async () => {
  const labelWrites = [];
  const client = baseClient({
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    graphql: async () => ({
      data: {
        repository: {
          pullRequest: {
            closingIssuesReferences: {
              nodes: [{
                number: 44,
                labels: { nodes: [{ name: "Wait For PR" }, { name: "Status: In Progress" }] },
                closedByPullRequestsReferences: { nodes: [{ number: 12, state: "CLOSED" }, { number: 13, state: "OPEN" }] },
              }],
            },
          },
        },
      },
    }),
  });
  await runPrAutomation({ client, event: { action: "closed", pull_request: pull() } });
  assert.deepEqual(labelWrites, []);
});

test("merged PR marks itself and clears linked issue lifecycle when it is the last PR", async () => {
  const labelWrites = [];
  const client = baseClient({
    getPull: async () => pull({ merged: true, labels: [{ name: "Rust" }, { name: "Status: In Progress" }, { name: "CI: Running" }, { name: "CI: Passed" }] }),
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
    graphql: async () => ({
      data: {
        repository: {
          pullRequest: {
            closingIssuesReferences: {
              nodes: [{
                number: 44,
                labels: { nodes: [{ name: "Bug" }, { name: "Wait For PR" }, { name: "Status: In Progress" }] },
                closedByPullRequestsReferences: { nodes: [{ number: 12, state: "MERGED" }] },
              }],
            },
          },
        },
      },
    }),
  });
  await runPrAutomation({
    client,
    event: { action: "closed", pull_request: pull() },
  });
  assert.deepEqual(new Set(labelWrites[0].labels), new Set(["Rust", "Merged"]));
  assert.deepEqual(labelWrites[1], { number: 44, labels: ["Bug"] });
});

test("closed unmerged PR clears its CI lifecycle labels", async () => {
  const labelWrites = [];
  const client = baseClient({
    getPull: async () => pull({ labels: [{ name: "Rust" }, { name: "CI: Running" }, { name: "CI: Passed" }] }),
    setIssueLabels: async (number, labels) => labelWrites.push({ number, labels }),
  });
  await runPrAutomation({ client, event: { action: "closed", pull_request: pull() } });
  assert.deepEqual(labelWrites[0], { number: 12, labels: ["Rust"] });
});
