#!/usr/bin/env node

import { readFile } from "node:fs/promises";

import { expectedWorkflows, labelsForFiles } from "./automation-core.mjs";
import {
  AUTOMATION_LABELS,
  CI_COMMENT_MARKER,
  MANAGED_PR_LABELS,
  PR_GATE_NAME,
} from "./automation-config.mjs";
import { GitHubClient } from "./github-client.mjs";

function linkedIssueQuery() {
  return `query($owner: String!, $repo: String!, $number: Int!) {
    repository(owner: $owner, name: $repo) {
      pullRequest(number: $number) {
        closingIssuesReferences(first: 50) {
          nodes {
            number
            labels(first: 50) { nodes { name } }
            closedByPullRequestsReferences(first: 50) { nodes { number state } }
          }
        }
      }
    }
  }`;
}

async function getLinkedIssues(client, number) {
  const response = await client.graphql(linkedIssueQuery(), {
    owner: client.owner,
    repo: client.repo,
    number,
  });
  return response.data.repository.pullRequest.closingIssuesReferences.nodes;
}

function inheritedPriority(issues) {
  const priorities = new Set(
    issues.flatMap((issue) => issue.labels.nodes.map((label) => label.name))
      .filter((name) => name.startsWith("Priority: ")),
  );
  return priorities.size === 1 ? [...priorities][0] : null;
}

async function synchronizeOpenPull(client, pull) {
  const files = (await client.listPullFiles(pull.number)).map((file) => file.filename);
  const desiredManaged = new Set(labelsForFiles(files));
  const currentLabels = pull.labels.map((label) => label.name);
  const preserved = currentLabels.filter((label) => !MANAGED_PR_LABELS.includes(label));
  const linkedIssues = await getLinkedIssues(client, pull.number);
  const priority = inheritedPriority(linkedIssues);
  if (priority && !preserved.some((label) => label.startsWith("Priority: "))) preserved.push(priority);
  await client.setIssueLabels(pull.number, [...preserved, ...desiredManaged]);

  const existingGate = (await client.listCheckRuns(pull.head.sha))
    .filter((run) => run.name === PR_GATE_NAME)
    .sort((left, right) => right.id - left.id)[0];
  if (!existingGate) {
    const required = expectedWorkflows(files);
    const body = {
      name: PR_GATE_NAME,
      head_sha: pull.head.sha,
      status: required.length === 0 ? "completed" : "in_progress",
      output: {
        title: required.length === 0 ? "No path-filtered CI required" : "Waiting for required CI",
        summary: required.length === 0
          ? "No path-filtered CI workflows are required for this change."
          : required.map((name) => `${name}: pending`).join("\n"),
      },
    };
    if (required.length === 0) body.conclusion = "success";
    await client.createCheckRun(body);
  }

  if (expectedWorkflows(files).length === 0) {
    const comments = await client.listIssueComments(pull.number);
    const diagnostics = comments.find((comment) => comment.body?.includes(CI_COMMENT_MARKER));
    if (diagnostics) {
      await client.updateIssueComment(
        diagnostics.id,
        `${CI_COMMENT_MARKER}\n## CI diagnostics resolved for \`${pull.head.sha.slice(0, 12)}\`\n\nNo path-filtered CI workflows are required for the latest PR head.\n\n_This comment is updated in place for the latest PR head._`,
      );
    }
  }

  for (const issue of linkedIssues) {
    const labels = issue.labels.nodes.map((label) => label.name);
    const next = labels.filter((label) => label !== "Status: Ready");
    if (!next.includes("Wait For PR")) next.push("Wait For PR");
    if (!next.some((label) => label.startsWith("Status: "))) next.push("Status: In Progress");
    await client.setIssueLabels(issue.number, next);
  }
}

async function synchronizeClosedPull(client, pull) {
  if (pull.merged) {
    const labels = pull.labels.map((label) => label.name)
      .filter((label) => !["Status: In Progress", "Status: Needs Decision"].includes(label));
    if (!labels.includes("Merged")) labels.push("Merged");
    await client.setIssueLabels(pull.number, labels);
  }

  for (const issue of await getLinkedIssues(client, pull.number)) {
    const hasAnotherOpenPull = issue.closedByPullRequestsReferences.nodes.some(
      (candidate) => candidate.number !== pull.number && candidate.state === "OPEN",
    );
    if (hasAnotherOpenPull) continue;
    const next = issue.labels.nodes.map((label) => label.name)
      .filter((label) => !["Wait For PR", "Status: In Progress"].includes(label));
    await client.setIssueLabels(issue.number, next);
  }
}

export async function runPrAutomation({ client, event }) {
  for (const [name, definition] of Object.entries(AUTOMATION_LABELS)) {
    await client.ensureLabel(name, definition);
  }
  const eventPull = event.pull_request;
  if (!eventPull) throw new Error("pull_request payload is required");
  const pull = await client.getPull(eventPull.number);
  if (event.action === "closed") return synchronizeClosedPull(client, pull);
  return synchronizeOpenPull(client, pull);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  const client = new GitHubClient({
    token: process.env.GITHUB_TOKEN,
    repository: process.env.GITHUB_REPOSITORY,
    apiUrl: process.env.GITHUB_API_URL,
  });
  await runPrAutomation({ client, event });
}
