export class GitHubClient {
  constructor({ token, repository, apiUrl = "https://api.github.com" }) {
    if (!token) throw new Error("GITHUB_TOKEN is required");
    if (!repository?.includes("/")) throw new Error("GITHUB_REPOSITORY must be owner/repo");
    [this.owner, this.repo] = repository.split("/", 2);
    this.apiUrl = apiUrl.replace(/\/$/, "");
    this.headers = {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "asterdrive-repository-automation",
    };
  }

  async request(method, path, body) {
    const response = await fetch(`${this.apiUrl}${path}`, {
      method,
      headers: this.headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (response.status === 204) return null;
    const text = await response.text();
    const payload = text ? JSON.parse(text) : null;
    if (!response.ok) {
      throw new Error(`${method} ${path} failed (${response.status}): ${payload?.message || text}`);
    }
    return payload;
  }

  async paginate(path, collectionKey = null) {
    const separator = path.includes("?") ? "&" : "?";
    const items = [];
    for (let page = 1; ; page += 1) {
      const payload = await this.request("GET", `${path}${separator}per_page=100&page=${page}`);
      const batch = collectionKey ? payload[collectionKey] : payload;
      if (!Array.isArray(batch)) {
        throw new Error(`Expected an array from ${path}${collectionKey ? ` at ${collectionKey}` : ""}`);
      }
      items.push(...batch);
      if (batch.length < 100) return items;
    }
  }

  repoPath(path) {
    return `/repos/${this.owner}/${this.repo}${path}`;
  }

  async ensureLabel(name, definition) {
    try {
      await this.request("GET", this.repoPath(`/labels/${encodeURIComponent(name)}`));
    } catch (error) {
      if (!error.message.includes("(404)")) throw error;
      await this.request("POST", this.repoPath("/labels"), { name, ...definition });
    }
  }

  listPullFiles(number) {
    return this.paginate(this.repoPath(`/pulls/${number}/files`));
  }

  getPull(number) {
    return this.request("GET", this.repoPath(`/pulls/${number}`));
  }

  async setIssueLabels(number, labels) {
    return this.request("PUT", this.repoPath(`/issues/${number}/labels`), { labels: [...new Set(labels)].sort() });
  }

  addIssueLabels(number, labels) {
    return this.request("POST", this.repoPath(`/issues/${number}/labels`), { labels });
  }

  listIssueComments(number) {
    return this.paginate(this.repoPath(`/issues/${number}/comments`));
  }

  createIssueComment(number, body) {
    return this.request("POST", this.repoPath(`/issues/${number}/comments`), { body });
  }

  updateIssueComment(commentId, body) {
    return this.request("PATCH", this.repoPath(`/issues/comments/${commentId}`), { body });
  }

  listCheckRuns(sha) {
    return this.paginate(this.repoPath(`/commits/${sha}/check-runs`), "check_runs");
  }

  getWorkflowRun(runId) {
    return this.request("GET", this.repoPath(`/actions/runs/${runId}`));
  }

  listWorkflowRunJobs(runId) {
    return this.paginate(this.repoPath(`/actions/runs/${runId}/jobs`), "jobs");
  }

  listPullsForCommit(sha) {
    return this.paginate(this.repoPath(`/commits/${sha}/pulls`));
  }

  createCheckRun(body) {
    return this.request("POST", this.repoPath("/check-runs"), body);
  }

  updateCheckRun(id, body) {
    return this.request("PATCH", this.repoPath(`/check-runs/${id}`), body);
  }

  listOpenIssues(labels = []) {
    const query = labels.length > 0 ? `&labels=${encodeURIComponent(labels.join(","))}` : "";
    return this.paginate(this.repoPath(`/issues?state=open${query}`));
  }

  createIssue(body) {
    return this.request("POST", this.repoPath("/issues"), body);
  }

  updateIssue(number, body) {
    return this.request("PATCH", this.repoPath(`/issues/${number}`), body);
  }

  async graphql(query, variables) {
    return this.request("POST", "/graphql", { query, variables });
  }
}
