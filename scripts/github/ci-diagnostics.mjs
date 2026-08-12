#!/usr/bin/env node

import { readFile } from "node:fs/promises";

import {
  classifyCheckRuns,
  expectedWorkflows,
  gateConclusion,
  incidentFingerprint,
  incidentMarker,
  incidentMatchesWorkflow,
  isInfrastructureFailure,
  parseIncidentState,
  renderDiagnosticsComment,
  renderIncidentBody,
} from "./automation-core.mjs";
import {
  AUTOMATION_LABELS,
  CI_COMMENT_MARKER,
  PR_GATE_NAME,
} from "./automation-config.mjs";
import { GitHubClient } from "./github-client.mjs";

function normalizeJob(job) {
  return {
    name: job.name,
    status: job.status,
    conclusion: job.conclusion,
    htmlUrl: job.html_url,
    steps: job.steps || [],
  };
}

async function collectWorkflowRuns(client, sha, expectedNames) {
  const checkRuns = await client.listCheckRuns(sha);
  const workflowRuns = new Map();
  for (const check of checkRuns) {
    const workflowName = check.check_suite?.app?.slug === "github-actions"
      ? check.name
      : null;
    const details = check.details_url || "";
    const match = details.match(/\/actions\/runs\/(\d+)/);
    if (!match) continue;
    const runId = Number(match[1]);
    if (workflowRuns.has(runId)) continue;
    const run = await client.getWorkflowRun(runId);
    if (!expectedNames.includes(run.name) || run.head_sha !== sha) continue;
    const jobs = (await client.listWorkflowRunJobs(runId)).map(normalizeJob);
    workflowRuns.set(runId, {
      workflowName: run.name,
      status: run.status,
      conclusion: run.conclusion,
      startedAt: run.run_started_at || run.created_at,
      htmlUrl: run.html_url,
      jobs,
      _workflowNameHint: workflowName,
    });
  }
  return [...workflowRuns.values()];
}

async function upsertGate(client, sha, workflows) {
  const state = gateConclusion(workflows);
  const existing = (await client.listCheckRuns(sha))
    .filter((run) => run.name === PR_GATE_NAME)
    .sort((left, right) => right.id - left.id)[0];
  const summary = workflows.length === 0
    ? "No path-filtered CI workflows are required for this change."
    : workflows.map((workflow) => `${workflow.name}: ${workflow.state}`).join("\n");
  const body = {
    name: PR_GATE_NAME,
    head_sha: sha,
    status: state.status,
    output: {
      title: state.conclusion === "success" ? "Required CI completed" : state.conclusion === "failure" ? "Required CI failed" : "Waiting for required CI",
      summary,
    },
  };
  if (state.conclusion) body.conclusion = state.conclusion;
  if (existing) {
    if (existing.status === "completed") {
      if (state.status === "completed" && existing.conclusion === state.conclusion) return existing;
      return client.createCheckRun(body);
    }
    delete body.name;
    delete body.head_sha;
    return client.updateCheckRun(existing.id, body);
  }
  return client.createCheckRun(body);
}

async function updatePrDiagnostics(client, pull) {
  const current = await client.getPull(pull.number);
  const sha = current.head.sha;
  const files = (await client.listPullFiles(pull.number)).map((file) => file.filename);
  const expected = expectedWorkflows(files);
  const workflows = classifyCheckRuns(await collectWorkflowRuns(client, sha, expected), expected);
  await upsertGate(client, sha, workflows);

  const comments = await client.listIssueComments(pull.number);
  const existing = comments.find((comment) => comment.body?.includes(CI_COMMENT_MARKER));
  const hasProblem = workflows.some((workflow) => ["failure", "cancelled"].includes(workflow.state));
  if (!hasProblem && !existing) return;
  const body = renderDiagnosticsComment({ sha, workflows });
  if (existing) await client.updateIssueComment(existing.id, body);
  else await client.createIssueComment(pull.number, body);
}

async function updateIncidentOnFailure(client, run, jobs) {
  const failedJobs = jobs.filter((job) => ["failure", "timed_out", "action_required"].includes(job.conclusion));
  if (failedJobs.length === 0) {
    failedJobs.push({
      name: "Workflow-level failure",
      status: run.status,
      conclusion: run.conclusion,
      htmlUrl: run.html_url,
      steps: [],
    });
  }
  const fingerprint = incidentFingerprint({
    workflowName: run.name,
    branch: run.head_branch,
    failedJobs,
  });
  const marker = incidentMarker(fingerprint);
  const issues = await client.listOpenIssues(["CI: Failure"]);
  const relatedIssues = issues.filter((issue) =>
    !issue.pull_request && incidentMatchesWorkflow(issue.body, run.name, run.head_branch),
  );
  const existing = issues.find((issue) => issue.body?.includes(marker));
  const state = existing ? parseIncidentState(existing.body) : { occurrences: 0, recoveryStreak: 0 };
  const body = renderIncidentBody({
    fingerprint,
    workflowName: run.name,
    branch: run.head_branch,
    runUrl: run.html_url,
    sha: run.head_sha,
    failedJobs,
    occurrences: state.occurrences + 1,
    recoveryStreak: 0,
  });
  const labels = ["CI: Failure"];
  if (isInfrastructureFailure(failedJobs)) labels.push("CI: Infrastructure");
  for (const issue of relatedIssues) {
    if (issue.number === existing?.number) continue;
    const resetBody = issue.body.replace(
      /\| Recovery streak \| \d+ \/ 2 \|/,
      "| Recovery streak | 0 / 2 |",
    );
    if (resetBody !== issue.body) await client.updateIssue(issue.number, { body: resetBody });
  }
  if (existing) await client.updateIssue(existing.number, { body, labels });
  else await client.createIssue({ title: `[CI] ${run.name} failed on ${run.head_branch}`, body, labels });
}

async function updateIncidentOnSuccess(client, run) {
  const issues = await client.listOpenIssues(["CI: Failure"]);
  const matching = issues.filter((issue) =>
    !issue.pull_request && incidentMatchesWorkflow(issue.body, run.name, run.head_branch),
  );
  for (const issue of matching) {
    const state = parseIncidentState(issue.body);
    const nextStreak = state.recoveryStreak + 1;
    const body = issue.body.replace(
      /\| Recovery streak \| \d+ \/ 2 \|/,
      `| Recovery streak | ${nextStreak} / 2 |`,
    );
    await client.updateIssue(issue.number, {
      body,
      state: nextStreak >= 2 ? "closed" : "open",
      state_reason: nextStreak >= 2 ? "completed" : undefined,
    });
  }
}

export async function runCiDiagnostics({ client, event }) {
  for (const [name, definition] of Object.entries(AUTOMATION_LABELS)) {
    await client.ensureLabel(name, definition);
  }
  const run = event.workflow_run;
  if (!run) throw new Error("workflow_run payload is required");
  const candidates = run.pull_requests?.length > 0
    ? run.pull_requests
    : await client.listPullsForCommit(run.head_sha);
  const openPulls = [];
  for (const candidate of candidates) {
    const pull = await client.getPull(candidate.number);
    if (pull.state === "open" && pull.head.sha === run.head_sha) openPulls.push(pull);
  }
  if (openPulls.length > 0 && run.event === "pull_request") {
    for (const pull of openPulls) await updatePrDiagnostics(client, pull);
    return;
  }

  const repositoryDefaultBranch = event.repository.default_branch;
  if (run.event !== "schedule" && run.head_branch !== repositoryDefaultBranch) return;
  const jobs = (await client.listWorkflowRunJobs(run.id)).map(normalizeJob);
  if (["failure", "timed_out", "action_required"].includes(run.conclusion)) {
    await updateIncidentOnFailure(client, run, jobs);
  } else if (run.conclusion === "success") {
    await updateIncidentOnSuccess(client, run);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  const client = new GitHubClient({
    token: process.env.GITHUB_TOKEN,
    repository: process.env.GITHUB_REPOSITORY,
    apiUrl: process.env.GITHUB_API_URL,
  });
  await runCiDiagnostics({ client, event });
}
