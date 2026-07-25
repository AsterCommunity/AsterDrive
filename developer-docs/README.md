# AsterDrive Developer Documentation

This directory is the source library for AsterDrive’s developer-facing documentation. The published site is available at [drive.astercosm.com/developer/](https://drive.astercosm.com/developer/); deployment and end-user documentation remains under [`docs/`](../docs/).

## Languages

- [简体中文](./zh-CN/README.md)
- [English](./en/README.md)

## Library structure

Each language follows the same information architecture:

```text
architecture/  Repository architecture, module boundaries, and service ownership
design/        Domain design notes and cross-layer contracts
api/           REST, WebDAV, WOPI, and internal protocol reference
testing/       Test infrastructure, compliance checks, and diagnostics
records/       Draft notes and historical decision snapshots
```

Current implementation documents are authoritative only when they agree with the current code. Files under `records/` preserve drafts or historical context and must state their status explicitly.

## Editing and publishing

- Edit the Markdown sources in this directory; do not edit the generated `docs/src-developer/` tree.
- `cd docs && bun run developer-docs:build` builds the independent developer site under `/developer/`.
- Relative links should target source Markdown files so they work on GitHub; the build preparation step maps them to published routes.
