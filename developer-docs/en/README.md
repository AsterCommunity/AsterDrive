# Developer Docs

This directory contains developer-facing documentation, including API references and architecture notes.

The `docs/` tree is reserved for deployment-facing and end-user documentation, so these developer docs are excluded from the user documentation site build.

## Languages

- [Simplified Chinese](../zh-CN/README.md)
- English

## Entry Points

- [Architecture Overview](./architecture.md)
- [Core Module Design Notes](./module-designs.md)
- [Backend Service Ownership Boundaries](./backend-service-ownership.md)
- [External Authentication Module](./external-auth.md)
- [Storage Descriptor and Field Normalization Contract](./storage-descriptor-normalization-contract.md)
- [Object Naming and OneDrive Direct Downloads](./storage-object-naming-and-onedrive-direct-download.md)
- [Upload Finalization Contracts](./upload-finalization-contracts.md)
- [API Overview](./api/index.md)
- [Tags API](./api/tags.md)
- [Testing and Database Backends](./testing.md)
- [WebDAV Conformance and Compatibility Testing](./webdav-compliance-testing.md)
- [Jemalloc Heap Profiling](./jemalloc-profiling.md)

## Current Status

These documents track the current codebase implementation by default. Documents with an explicit draft or historical-plan status record decision context or follow-up work and must not be treated as the current implemented directory structure, API surface, or acceptance state.
