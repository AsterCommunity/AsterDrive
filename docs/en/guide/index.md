---
description: Overview of the AsterDrive user guide, organized by getting started, daily use, admin configuration, operations, and reference content.
---

# User Guide

This documentation set is organized by what you are trying to do, not by feature names you have to memorize.

If this is your first time here, start with [Getting Started](./getting-started). If your service is already running, jump directly to the section that matches your role.

## First Deployment

If you just want to get the service running first, read these pages:

- [Getting Started](./getting-started): run through login, upload, sharing, and WebDAV with the fewest steps
- [Deployment Overview](/en/deployment/): choose Docker, systemd, or a direct binary run before going live
- [First-Start Checklist](/en/deployment/runtime-behavior): confirm default configuration, storage policies, health checks, and background tasks
- [Reverse Proxy](/en/deployment/reverse-proxy): read this before adding HTTPS, a domain name, WebDAV, or WOPI

## Daily Use

After the service opens normally, regular users should start here:

- [User Manual](./user-guide): files, workspaces, trash, shares, WebDAV, and personal settings
- [Common Workflows](./core-workflows): common operations connected through real scenarios
- [Teams and Permissions](./teams-and-permissions): personal spaces, team spaces, team roles, and admin boundaries
- [Sharing and Public Access](./sharing): share links, passwords, expiration, and download limits
- [File Editing](./editing): in-browser editing, version history, and WOPI open methods
- [Online Preview and WOPI](./preview-and-wopi): OnlyOffice, Collabora, and WOPI open-method integration
- [Uploads and Large Files](./upload-modes): resumable uploads, direct-to-object-storage uploads, and failure diagnosis

## Feature Map

If you are not looking by task, but by backend capability and ownership boundary, read the [Feature Map](/en/features/).

It connects identity and access, files and workspaces, uploads and storage, preview and processing, and system operations. It is useful for administrators, troubleshooting, and extension work.

## Administrators

Administrators should separate three entry points first: the admin UI, runtime system settings, and the startup configuration file.

- [Admin Console](./admin-console): what each admin page is responsible for
- [Configuration Overview](/en/config/): what `config.toml`, system settings, storage policies, and external proxies each control
- [System Settings](/en/config/runtime): site, registration, cookies, mail, scheduling, trash, WOPI, and audit logs
- [External Authentication](/en/config/external-auth): connect OpenID Connect, Generic OAuth2, Logto, GitHub, QQ, Google, or Microsoft login
- [Storage Policies](/en/config/storage): storage types, policy groups, and existing data migration
- [Storage Backend Details](/en/storage/): backend-specific tutorials and options
- [Follower Nodes](./remote-nodes): connect another AsterDrive instance as a remote storage backend
- [Custom Frontend](./custom-frontend): replace frontend assets, inject custom configuration, and handle CSP

## Operations

After launch, stable operation matters more than whether the page can open. Prepare your checks, backup, upgrade, and troubleshooting paths ahead of time.

- [Operations CLI](/en/deployment/ops-cli): `doctor`, offline system settings, cross-database migration, and node enrollment
- [Production Checklist](/en/deployment/production-checklist): final HTTPS, data, backup, storage, and real-feature checks before launch
- [Upgrades and Version Migration](/en/deployment/upgrade): backup before upgrading, verify after upgrading, and roll back after failures
- [Backup and Restore](/en/deployment/backup): database, configuration, local upload directory, and restore order
- [Troubleshooting](/en/deployment/troubleshooting): startup, upload, download, sharing, WebDAV, WOPI, and background tasks
- [Performance Benchmarking](/en/deployment/performance-benchmarking): establish a local baseline and rerun smoke tests

## The Project Itself

To understand why AsterDrive is designed this way, who it is for, and who it is not for, read [About AsterDrive](/en/reference/about).

To build the system mental model first, read [Architecture Overview](/en/reference/architecture). It covers primary / follower nodes, component relationships, upload and download data flows, and troubleshooting entry points.

If a concept is unclear, start with the [Glossary](/en/reference/glossary). When you hit a problem, check the [FAQ Triage](/en/reference/faq) first. If you have an error code, read [Error Code Handling](/en/reference/errors). If the problem happens during deployment, reverse proxying, WebDAV, or WOPI, use it together with [Troubleshooting](/en/deployment/troubleshooting).
