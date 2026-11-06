# Workspace and resource authorization

This contract implements #745. Drive owns product authorization in `src/services/workspace/scope/`. REST, WebDAV, WOPI, shares, background tasks and governed administrator access use these rules while retaining their authentication protocols and response formats.

## Inputs and grants

`WorkspaceStorageScope` identifies an actor and requested workspace; it is not proof of permission. `WorkspaceAccessConstraints` carries a delegated root, WebDAV account/verifier binding or WOPI session and relative-target restrictions. `WorkspaceOperation` declares the product action. Future RBAC (#663) belongs at this decision boundary, without duplicating role interpretation in protocol adapters.

`authorize_workspace_on` checks current actor status, team lifecycle/membership and supplied credential/session constraints against authoritative data. `AuthorizedWorkspaceAccess` borrows the database connection for this operation. Mutation callers must use their actual writer transaction and obtain a new grant on transaction retry. Grants are never serialized, cached or stored in tasks.

`AuthorizedFileCreation` binds the transaction, actor, workspace and final parent collection. The record writer cannot accept a second, replaceable scope or parent. `AuthorizedResourceLock` binds a canonical lock root and namespace; token mutation consumes it on the same transaction. `AuthorizedResourceRead` distinguishes user, share and governed administrator access. Shared metadata, revision and trash projections require this proof instead of trusting a caller-supplied owner ID.

Repositories provide facts, locking reads and atomic SQL. Typed grants and module visibility prevent common internal mistakes; they do not magically authorize arbitrary raw SQL.

## Operation matrix

| Operation | Required scope | Execution boundary |
| --- | --- | --- |
| Metadata, content, revisions, trash | Current authority, workspace, entity and delegated/public capability | Authorization uses writer; stale-tolerant content projections may use reader |
| Icon, properties, metadata | Current actor authority and target | Authorization and resource locks within writer transaction |
| Empty files, directories, relative upload paths | Workspace, destination collection and each created path segment | Authorization precedes resource/provider/session side effects; replay also checks current visibility |
| Upload init/finalization | Actor, session ownership, current target and credential root | Admission first; reauthorize after body receipt/staging and before publication; check destination collection or overwrite target |
| Copy | Source read scope and destination creation scope | Validate each destination collection |
| Rename/move | Target, source and destination collections | Same-parent rename also changes member URLs; Infinity operations check canonical descendant locks |
| Delete/restore | Target and actual source/destination collection | Validate final parent, including fallback to workspace root; background paths follow the same rule |
| Lock lifecycle/discovery | Workspace, canonical root and mount bounds | A token does not authorize another workspace/root; invisible ancestor locks still constrain mutation without exposing their tokens |
| Task create/manual retry/cancel | Actor, target and task kind | Ownership does not retain former administrator privileges |
| Governed administrator reads/downloads | Current admin/session, access record, target and expiry | Preserve real actor, reason, version scope and required audit; no ordinary file mutation authority |

File/directory deletion no longer accepts an `allow_locked` boolean bypass. Protocol adapters parse positive token conditions and provide actual credentials; the product lock evaluator checks affected resources.

## Concurrency and authority

Mutations acquire the workspace namespace before owner/team, membership/actor/credential and resource rows, preserving quota, revision, lease and ordered-resource requirements. Team removal/archive uses the same team row as publication. Membership reads under that guard use current/locking reads, including on MySQL REPEATABLE READ.

Writer owns permission and revocation facts. WebDAV caches only the verifier associated with expensive password verification; every request checks current account, verifier, owner and team facts. Cache invalidation is an optimization: republishing a stale cache entry after revocation cannot restore authority.

Body receipt, downloads and provider staging do not hold a database transaction. Publication obtains a fresh grant and checks current ancestry. Moving the target outside a restricted root during transfer rejects publication and follows existing staging cleanup. Idempotent results require current visibility as well.

Revocation committed first rejects subsequent authorization/publication. A valid publication that already holds and completes its protected transaction retains its committed result. Revocation cannot recall delivered bytes.

## Canonical locks and protocol behavior

Lock identity consists of namespace, root kind and stable entity ID. Stored presentation paths are not authority. Coverage, descendant conflicts, discovery, cleanup and replacement rebinding use current resources and ancestry; identical `/same.txt` paths in different mounts are unrelated resources.

Bulk discovery loads root entities and ancestor frontiers together. Responses project canonical roots to mount-relative URIs, retaining collection trailing slashes. Tagged/untagged `If`, shared locks satisfied by a valid token for the same root, backend failures and protocol status mappings remain explicit contracts.

## Tasks and capabilities

User-delegated file tasks recheck current authority on execution and publication. Accepted system maintenance follows its system lifecycle and preserves original attribution. Manual restart/cancel of privileged tasks requires current administrator authority.

WOPI retains its session, file, app, proof and lock contracts. Publication rechecks the session and user version. PUT_RELATIVE derives authority only for the original file's selected parent collection, with current source location checked again.

Authentication adapters enforce ordinary request credential expiry. Final mutation checks current actor/membership/resource and supplied protocol restrictions. Existing read streams do not perform per-byte database checks. External presigned URLs retain provider-side TTLs; rejecting application publication does not immediately revoke an issued provider capability.

## Frontend operation ownership

Queued uploads freeze account, login generation, workspace and target. Every transport stage, poll, completion, retry and cancellation uses the bound service. Navigation cannot redirect an existing operation. Directory extraction captures its destination before awaiting.

Login lifecycle changes invalidate the generation; normal refresh preserves it. Logout immediately aborts old requests. Signals remain in HTTP configs to prevent refresh/replay with another account's cookies. XHR checks cancellation again before sending; late promises cannot start another stage or update a successor account.

Restored tasks use the upload host lifetime, independently of a discovery effect's lifetime. Persistence requires account/workspace ownership; legacy unknown-owner records cannot automatically execute. The server can rediscover the current account's recoverable sessions. Frontend binding never replaces server authorization.

## Maintenance and verification

Each new entry declares actor, action, source/destination bounds, authority database, decision point and audit. Internal interface changes migrate all consumers and remove replaced forwarding helpers and duplicate policies.

Regression coverage includes outsiders/removed members, administrator demotion, stale cache republication, cross-workspace/root tokens, moving targets during body receipt, resource-depth collection locks, canonical descendant locks, replay, delayed task publication and frontend navigation/account/unmount races. Database concurrency coverage includes SQLite, PostgreSQL and MySQL. Protocol coverage includes both allowed and denied operations.

#586 owns connection-account and credential lifecycle features; #663 owns the RBAC catalog and management UI. Independent password/MFA, external identity, XSS or SSRF defects are not automatically resolved by #745.

Logout synchronously clears private listings, teams, playback and downloads, including download controllers and retry closures. Team and folder-tree responses check their request lifecycle before installing results; an old finalizer cannot clear a new request. This does not resolve the separate session race where refresh cookies arrive after logout.
