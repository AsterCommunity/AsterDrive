# Maintaining external provider evidence

The [evidence index](../reference/storage-provider-evidence.md) is generated from `docs/storage-provider-evidence.json`. It owns external references; connector descriptors own advertised capabilities, and connector/driver tests own executable wire contracts. None proves the others. All built-in connectors are covered, including explicit internal contracts for local/remote and the SFTP v3 IETF draft (not an RFC).

## References and ownership

Use `provider:<id>/<topic>`, for example `provider:alibaba_oss/cors`. Topics are `objects`, `uploads`, `signing`, `cors`, `range_etag`, `errors`, `permissions`, and `domains`. Source headers and both tutorial languages link to stable provider anchors; topic links use `#<id>-<topic>`, for example `#alibaba_oss-cors`. Do not create a provider matrix in the generic frontend.

Storage maintainers own evidence, connectors, drivers and tests together; repository maintainers review when no dedicated CODEOWNERS assignment exists. New connectors must enter the live registry, generated connector manifest and evidence index. Future plugin manifest/design reviews must include these references, official sources, versions, permissions, request/response contracts and cloud acceptance status. This change does not invent runtime fields for a future plugin manifest.

## Evidence boundaries

Each provider records API/signature versions, exact SDK/transport versions from `Cargo.lock`, code and fixture paths, public configuration/credential field snapshots, critical protocol/limit guards, and bilingual conclusions for eight capabilities. Sources record official URLs, applicable versions, access attempts, content review dates and status. Pinned official SDK source supplements specifications; it cannot be the sole authority for an external cloud capability. Blogs, search snippets and generated clients are insufficient as sole sources.

- `verified` means a maintainer accessed and checked the cited content; `reviewed_on` records that review. It proves only document review.
- `unverified` means content review is incomplete, blocked by network/login/region/WAF, or insufficient. Keep `reviewed_on` null and explain why.
- Link report `reachable` means an official HTTPS URL returned 2xx this time. Title and critical limit markers reject soft 404s, login pages and unrelated tutorials; other content changes still require manual review. Link checks never update review dates.
- `cloud_acceptance.not_recorded` means this index asserts no cloud acceptance record. Local fixtures, old PRs and HTTP 200 do not establish acceptance. `recorded` requires a repository path to a sanitized acceptance record.

Access attempts and verified content reviews expire after 90 days. A new access attempt cannot refresh an old content review. When content review cannot be completed, retain the URL, mark unverified, clear reviewed_on and explain the failure. A failed link probe affects the reachability report and does not erase a content review completed through another successful request. Offline checks allow explicitly explained unverified sources and display their count. Live link checks return nonzero for any unreachable source and retain a detailed report.

## Workflow

```bash
node docs/scripts/storage-provider-evidence.mjs --write
make storage-provider-check
make storage-docs-check
make storage-provider-links
node --test docs/scripts/storage-provider-evidence.test.mjs
```

Edit curated JSON, then generate both index pages; do not hand-edit generated pages. Offline checks detect missing providers, references, field/version/critical-constant drift and stale evidence. `storage-docs-check` also runs the existing descriptor projection test. `storage-provider-links.json` is a temporary local/CI reachability report, not a content-review fact source. Weekly CI checks links and retains reports; PR offline checks do not depend on external availability. Relevant provider contract tests remain required: string guards cannot replace executable protocol tests.

For provider changes: first review official API, signature, permissions and limits and update evidence; then change driver/connector/descriptor/plugin contracts and all callers. Test ordinary requests and presigned URLs separately, including multipart/resumable, management CORS, Range/ETag, error mapping, permissions and independent signing vectors where needed. Distinguish provider limits from local resource caps. Deliberately refresh dependency pins, field snapshots and guards, generate the index, review bilingual tutorials, run provider tests, `make storage-docs-check`, live link checks and both documentation builds.

An **origin** serves object APIs; an **upload endpoint** serves upload protocols; a **bucket-bound CNAME** may alias origin APIs; a **CDN/custom download domain** serves downloads with independent private-origin access, client signatures, caches and CORS. Accept origin, upload, CNAME and CDN separately with positive/negative checks and cache misses/hits. Record missing credentials or unrun cloud checks explicitly. Successful CORS management never establishes browser upload or CDN acceptance.
