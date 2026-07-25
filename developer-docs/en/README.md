# AsterDrive Developer Documentation

This is the source-level documentation library for AsterDrive contributors, integration developers, and maintainers. It covers repository architecture, domain design, APIs, protocol contracts, test infrastructure, and diagnostic workflows. End users, deployers, and administrators should start with the [user documentation](https://drive.astercosm.com/en/).

## Where to start

| Goal | Start here |
| --- | --- |
| Build a mental model of the repository | [Architecture overview](./architecture/index.md) → [Core module design notes](./architecture/module-designs.md) |
| Decide which backend layer owns a change | [Backend service ownership boundaries](./architecture/backend-service-ownership.md) |
| Find REST, WebDAV, WOPI, or internal protocol behavior | [API overview](./api/index.md) |
| Change storage, upload, authentication, or remote-node behavior | [Domain design and contracts](./design/README.md) |
| Run database, WebDAV, or diagnostic checks | [Testing and diagnostics](./testing/index.md) |
| Read draft discussions or historical design context | [Draft and historical records](./records/README.md) |

## Documentation library

### Architecture and boundaries

- [Architecture overview](./architecture/index.md): node modes, layering, startup, configuration, and data flow.
- [Core module design notes](./architecture/module-designs.md): internal shapes of file, upload, task, storage, and protocol modules.
- [Backend service ownership boundaries](./architecture/backend-service-ownership.md): responsibilities of routes, services, domain modules, repositories, storage, and protocols.

### Domain design and contracts

- [Domain design and contract index](./design/README.md)
- [External authentication module](./design/external-auth.md)
- [Storage descriptor and field normalization](./design/storage-descriptor-normalization-contract.md)
- [Object naming and OneDrive direct downloads](./design/storage-object-naming-and-onedrive-direct-download.md)
- [Upload finalization contracts](./design/upload-finalization-contracts.md)
- [Remote storage target and policy ownership](../zh-CN/design/remote-storage-target-policy-ownership.md) — **translation pending**

### APIs and protocols

The [API overview](./api/index.md) links every endpoint page across authentication, file workflows, teams and sharing, background tasks, administration, WebDAV, WOPI, health checks, and the follower internal storage protocol. The generated OpenAPI document remains the machine-readable contract.

### Testing and diagnostics

- [Testing and database backends](./testing/index.md)
- [WebDAV conformance and compatibility testing](./testing/webdav-compliance-testing.md)
- [Jemalloc heap profiling](./testing/jemalloc-profiling.md)

### Draft and historical records

- [Draft and historical record index](./records/README.md)
- [Static configuration secret-handling memo](../zh-CN/records/static-config-secret-handling.md) — **draft, translation pending**
- [Service modularization refactor plan](../zh-CN/records/service-modularization-refactor-plan.md) — **historical snapshot, translation pending**

## Document status

| Status | Meaning |
| --- | --- |
| Current implementation | Default; expected to match current code, routes, and tests |
| Draft | A discussion direction that has not been accepted or completed |
| Historical snapshot | Decision context only; old names and paths are not current implementation guidance |
| Translation pending | The Chinese source is available and the English site may show Starlight fallback content |

Verify claims against the current code and focused tests before updating implementation. An old document path is not a reason to resurrect an obsolete compatibility layer.
