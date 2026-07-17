---
layout: home
description: The official AsterDrive documentation home, organized by quick start, user guides, administrator configuration, deployment, and operations, covering Docker, systemd, WebDAV, WOPI, follower nodes, and backup and restore.

hero:
  name: AsterDrive
  text: Official Documentation Center
  tagline: Built with Rust + React. Start with single-node deployment, then add team spaces, S3, Azure Blob, OneDrive, SFTP, WebDAV, WOPI, and follower nodes as needed.
  actions:
    - theme: brand
      text: Quick Start
      link: /en/guide/getting-started
    - theme: alt
      text: User Guides
      link: /en/guide/
    - theme: alt
      text: Deployment Overview
      link: /en/deployment/

features:
  - title: First Deployment
    details: From quick start to production, first get the service running, then handle HTTPS, data directories, health checks, and the first validation pass.
    link: /en/guide/getting-started
  - title: Daily Use
    details: Files, workspaces, uploads, sharing, trash, WebDAV, and online editing, organized around real usage paths.
    link: /en/guide/
  - title: Feature Map
    details: Organizes identity, files, uploads, preview, and operations by backend capability for administration, troubleshooting, and extension work.
    link: /en/features/
  - title: Administrator Configuration
    details: Separate the responsibilities of config.toml, admin-console system settings, storage policies, policy groups, storage backends, mail, and follower nodes.
    link: /en/config/
  - title: Operations and Maintenance
    details: Docker, systemd, reverse proxy, upgrades, backup, troubleshooting, and the operations CLI are grouped into one maintenance path.
    link: /en/deployment/
---

## First, Know What It Is

AsterDrive is a lightweight self-hosted cloud drive built with Rust and React. You can start with the default single-node deployment using SQLite and local storage, then connect PostgreSQL / MySQL, S3-compatible object storage, Azure Blob Storage, Tencent COS, Microsoft Graph-backed OneDrive / SharePoint drives, SFTP file servers, team spaces, WebDAV, WOPI online preview and editing, and follower node storage as needed.

It is not a full collaboration suite or a multi-primary cluster system. The current focus is making file management, sharing, uploads, previews, storage policies, and routine operations clear for individuals and small teams.

## Follow Your Goal

### I Just Want to Get It Running First

Start with [Quick Start](/en/guide/getting-started). It walks you through starting the service, creating the first administrator, uploading a file, trying sharing, checking WebDAV, and running a basic acceptance pass.

If you have already decided to deploy formally, go straight to [Deployment Overview](/en/deployment/). That documentation set explains Docker, systemd, reverse proxy, launch checks, upgrades, and backup along one path.

### I Have Logged In and Want to Know How to Use It

Start from [User Guides](/en/guide/). Regular users should first read the [User Manual](/en/guide/user-guide) and [Common Workflows](/en/guide/core-workflows), then jump to team permissions, sharing, editing, online preview, uploads, or WebDAV for specific questions.

### I Need to Manage an Instance

Read [Admin Console](/en/guide/admin-console) first, then [Configuration Overview](/en/config/). AsterDrive configuration is split into startup configuration, admin-console runtime settings, storage policies, policy groups, storage policy backends, and the external network environment. It is much clearer when viewed by layer.

If you are connecting S3 / MinIO / R2 / Azure Blob Storage / Tencent COS / OneDrive / SFTP, see [Storage Policy Backends](/en/storage/).

### I Want to Find Documentation by Feature Module

Read the [Feature Map](/en/features/). It connects identity, files, uploads, preview, and operations by backend capability, which is better for administration, troubleshooting, and extension work.

### I Need to Go Live or Troubleshoot

Before going live, choose a deployment method from [Deployment Overview](/en/deployment/), then add [Reverse Proxy](/en/deployment/reverse-proxy), [First-Start Checklist](/en/deployment/runtime-behavior), [Production Launch Checklist](/en/deployment/production-checklist), and [Backup and Restore](/en/deployment/backup). If something is already broken, go directly to [Troubleshooting](/en/deployment/troubleshooting), then combine it with [Error Code Handling](/en/reference/errors) when you see an error code.

### I Do Not Understand a Term, or I Do Not Know Where to Look

Start with the [Glossary](/en/reference/glossary) and [FAQ Quick Reference](/en/reference/faq). These two pages are not meant to be read from beginning to end; they are there to keep you from taking unnecessary detours.

---

::: tip In one sentence
**Do not add mental burden to your own data** - that is why we build AsterDrive.
:::
