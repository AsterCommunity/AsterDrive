# Documentation Contribution Guide

This page is for people preparing to change AsterDrive documentation. It covers both sites: the user documentation (`docs/`, published at <https://drive.docs.astercosm.com/>) and the developer documentation (`developer-docs/`, published at <https://drive.docs.astercosm.com/developer/>). We want every page to help readers complete one clear task, so before adding content, first confirm which reading path it belongs to.

## Decide Where It Belongs First

The user documentation is layered by reader task:

| What you are writing | Where it goes | Examples |
| --- | --- | --- |
| First use, quick start, deployment choices | `start/` | Quick start, common workflows, first admin |
| Daily operations, regular-user tasks | `using/` | Files, upload and download, sharing, using WebDAV, account security |
| Administrator scenario workflows | `admin/` | Users and teams, registration and SSO, mail, storage policies and policy groups, preview processing, offline download, custom frontend |
| Specific storage backend tutorials | `admin/storage-backends/` | Local disk, S3 / MinIO / R2, Azure Blob Storage, Tencent COS, OneDrive, SFTP, follower node storage policy |
| Deployment, launch, upgrade, backup, troubleshooting, monitoring | `deploy/` + `ops/` | Docker, systemd, reverse proxy, multi-instance, troubleshooting, operations CLI |
| `config.toml` fields, admin-console system settings | `reference/config/` | Server, database, deployment profile, each system-settings group |
| Concept explanations, capability matrices, protocol compatibility, indexes, problem routing | `reference/` | Runtime architecture, storage capability matrix, WebDAV protocol compatibility, glossary, error codes |
| Source modules, design contracts, internal protocol behavior | `developer-docs/` (this site) | Architecture overview, module designs, service ownership, design/ contracts |

When unsure, ask first: **what task did the reader open this page to complete?**

- "I want to use this feature" -> `using/`
- "I need to connect a specific backend, or walk through an admin scenario" -> `admin/`
- "I need to keep the service running steadily" -> `deploy/` + `ops/`
- "I need to change a setting or look up a field" -> `reference/config/`
- "I do not understand a term / do not know where to look / want to learn about the project" -> `reference/`
- "I want to change code and need module boundaries first" -> `developer-docs/`

## Adding Storage Backend Tutorials

Storage backend tutorials belong under `admin/storage-backends/` in the user documentation. Keep each page focused on one backend and follow the flow "prepare the backend service -> create a storage policy -> configure policy groups -> bind a test user or team -> validate".

For built-in connectors, runtime `StorageConnector` descriptors and connector localization are authoritative for identity, display name, deployment scope, credential mode, and transfer capabilities. `tests/storage_connector_docs.rs` reads that catalog through the admin APIs and generates:

- `docs/generated/storage-connectors.json`, the machine-readable manifest reviewed with each PR
- the backend selection table in `docs/src/content/docs/admin/storage-backends/index.md`
- the connector catalog in `docs/src/content/docs/admin/storage-policies.md`
- the capability matrix in `docs/src/content/docs/reference/storage-matrix.md`
- the corresponding blocks in all three English pages

`docs/astro.config.mts` builds the storage-backend sidebar directly from the manifest instead of maintaining another backend list. When adding or renaming a built-in connector:

1. Update its descriptor and localization plus the provider-owned tutorial slug / best-for summary in `tests/storage_connector_docs.rs`.
2. Add the Chinese and English provider tutorials.
3. Run `make storage-docs` and review the generated manifest and Markdown diff.
4. Run `make storage-docs-check`; CI runs the same drift check.

Generated blocks are bounded by `storage-connectors:*:start/end` markers and are not edited by hand. The backend overview, policy catalog, and capability matrix are exhaustive entry points. Provider names in READMEs, deployment explanations, troubleshooting, and tutorials are contextual examples and must say “for example”, “such as”, or “etc.” rather than implying a complete list. Contextual examples stay unchanged when a new connector is added.

If you only change details for one backend, do not copy large sections from another tutorial. Link common concepts to the storage policy and policy group page (`/admin/storage-policies/`) or the capability matrix (`/reference/storage-matrix/`).

## The Sidebar Is a Reading Flow

The user documentation site is built with Astro Starlight. There are no top-nav dropdown menus; site-wide navigation is the fixed sidebar, which does not switch by directory. Its goal is to keep readers aware of the whole documentation structure.

Prefer adding new pages into the fixed sidebar reading flow. Insert each page where readers first need it, and do not sort by filename.

Default order:

1. Start
2. Using
3. Administration
4. Deployment
5. Operations
6. Reference and Project

When adding a page, insert it where readers first need it. Do not sort by filename.

## Terminology Should Match the UI

Prefer using the product UI wording in documentation. When needed, add an English or internal name on first mention.

Recommended wording:

- `Follower Nodes`, and explain that they are followers when needed
- `Primary node`, and add `primary` when needed
- `Follower node`, and add `follower` when needed
- `Remote storage target`
- `Storage policy`
- `Policy group`
- `System settings`
- `Public site URL`
- `Preview app`
- `Audit log`

Avoid mixing multiple names on the same page, such as calling something "follower node", then "follower instance", then "remote storage instance". Explain it clearly once, then keep the same name.

## Help Readers Orient at the Start

Long pages should ideally start with three things:

- What the page covers
- When to read it
- Where to operate, or which quick-reference table to read first

Recommended structure (the page title lives in frontmatter; the body starts from level-2 headings):

```md
---
title: Page Title
---

:::tip[What this page covers]
One sentence defining the boundary. Avoid repeating large parts of adjacent pages here.
:::

## Entry Quick Reference

| What you want to do | Where to go |
| --- | --- |
| ... | ... |
```

## Link Rules

Prefer absolute paths for site links in the user documentation:

```md
[System Settings](/en/reference/config/runtime/)
[Follower Nodes](/en/admin/follower-nodes/)
[Troubleshooting](/en/ops/troubleshooting/)
```

Same-directory short links are also fine, but avoid relative paths such as `../guide/...` across directories. Absolute paths are easier to read and more stable when files move later.

Inside the developer documentation, use relative `.md` paths (they also work on GitHub); the build script maps them to published routes. Links from the developer documentation back to the user documentation use full URLs (`https://drive.docs.astercosm.com/...`).

## Writing Rules

- Give the conclusion first, then details
- Use tables for quick reference and lists for steps
- Use backticks for configuration items, paths, and commands
- Use `:::caution[title]` for dangerous operations
- Use `<details><summary>title</summary>` for optional background knowledge
- Do not write promises for features that have not been merged
- Do not copy large sections from another page just to be "complete"; link to that page instead

## Flow Diagram Rules

For flows, topologies, and data paths, prefer Mermaid:

```mermaid
flowchart TD
  Action["User action"] --> Decision{"System decision"}
  Decision --> ResultA["Result A"]
  Decision --> ResultB["Result B"]
```

For simple admin entry points, paths, configuration values, and command output, keep using `text` code blocks. Do not turn a single-line hint into a diagram.

Mermaid diagrams support click-to-zoom by default. Keep the normal document view compact: use short node labels, and put long explanations in the surrounding prose instead of inside nodes.

## Extra Rules for the Developer Documentation

The developer documentation (`developer-docs/`) differs from the user documentation in a few ways:

- Source files start with a single `# H1` heading and **no frontmatter**; the build script extracts the title and the first paragraph as the page title and description.
- Internal links use relative `.md` paths and are mapped to `/developer/` routes at build time.
- The `records/` directory holds drafts and historical snapshots. Files there must state their status explicitly (draft / historical snapshot) and are not authority for current implementation.
- Verify with `bun run developer-docs:build`, not `docs:build`.

## How Versioning Works

The user documentation is versioned by branch, not by build snapshots:

- Each released minor version's docs live on a `release/x.y` branch. The root path `/` serves the newest release branch, `/vX.Y/` serves older versions, and `/next/` serves the master development version
- On every release, CI automatically cuts the matching `release/x.y` branch from the tag. Any docs change pushed to `master` or `release/**` triggers a full rebuild of every version, so navigation and the version switcher stay current everywhere
- To fix docs for an old version, commit (or cherry-pick) directly to its `release/x.y` branch; CI rebuilds that version. Ancient versions without a branch (such as 0.1 and 0.2) are built automatically from the last tag of that minor line
- The version list is resolved entirely from git (`docs/scripts/resolve-versions.sh`): tags define which versions exist, and a `release/x.y` branch takes precedence over the tag when present. No static version table is maintained anywhere
- To preview the full versioned site locally:

```bash
bun run docs:preview:all
```

It builds every version with the same logic as CI (`/next/` uses your current working tree, including uncommitted changes) and starts a local preview.

## Chinese-English Sync Policy

The Chinese version is the source of truth. The English version is allowed to lag behind.

- A PR that only changes the Chinese version is acceptable, but note "English version not synced" in the PR description so maintainers can catch up later
- When you change technical facts (ports, paths, configuration keys, error codes, version numbers), both languages must be updated together. Never leave a stale value on one side
- If you are unsure about the English wording, change only the Chinese side rather than writing inconsistent facts on both sides

## Error Code Changes Must Pass the Check

When changing `src/api/api_error_code.rs` or `errors.md`, run locally first:

```bash
bun docs/scripts/check-error-codes.mjs
```

It compares the full set of error codes in the source against the error code documentation: referencing a non-existent code fails the check, and codes added in source but not mentioned in the docs are listed as warnings. CI runs the same check for changes to these paths.

## Verify After Changes

After changing the user documentation, run at least:

```bash
bun run docs:build
```

After changing the developer documentation, run at least:

```bash
bun run developer-docs:build
```

If you changed navigation, logo, sidebar, or the homepage, it is better to also run:

```bash
bun run docs:dev
```

Then click through:

- Homepage entry points
- Fixed sidebar collapse
- New pages
- Edit-this-page links
- Dark / light logos

Successful build is only the baseline. You still need to preview it yourself and confirm readers can follow the entry points and sidebar to find the content.
