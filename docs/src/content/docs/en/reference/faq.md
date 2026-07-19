---
title: "FAQ Triage"
---

This page is not a full troubleshooting manual. It routes symptoms to the right document. When something is already broken, finding the right entry point by symptom is faster than reading the whole manual from the beginning.

## Service and Login

| Symptom | Read first | Common causes |
| --- | --- | --- |
| The service will not start | [Troubleshooting: service will not start](/en/deployment/troubleshooting/#service-does-not-start) | Configuration file path, database connection, port conflict, directory permissions |
| Health check fails | [First-Start Checklist](/en/deployment/runtime-behavior/) | Database not ready, migrations incomplete, default policy not initialized |
| You keep getting logged out | [Login and Sessions](/en/config/auth/) / [System Settings](/en/config/runtime/#authentication-and-cookies) | Cookie HTTPS settings, public site URL, reverse proxy Host handling |
| New users cannot log in after registration | [System Settings](/en/config/runtime/#user-management) / [Mail](/en/config/mail/) | Email activation is enabled but mail delivery is not working |

## Uploads, Downloads, and Storage

| Symptom | Read first | Common causes |
| --- | --- | --- |
| Small files upload, large files fail | [Uploads and Large Files](/en/guide/upload-modes/) | Reverse proxy size limit, timeout, chunk size, temporary directory space |
| Direct-to-object-storage upload fails | [Storage Policies](/en/config/storage/) / [Uploads and Large Files](/en/guide/upload-modes/) | S3 CORS, exposed `ETag`, browser origin not allowed |
| Follower-node policy upload fails | [Follower Nodes](/en/guide/remote-nodes/) | Transport not reachable, wrong direct URL, default remote storage target not applied |
| Capacity looks wrong | [Operations CLI: doctor](/en/deployment/ops-cli/#deployment-check-doctor) | Storage usage counters drifted and need a deep check |

## Sharing, WebDAV, and Online Editing

| Symptom | Read first | Common causes |
| --- | --- | --- |
| Share link uses the wrong domain | [System Settings: public site URL](/en/config/runtime/#site-configuration) | Public site URL is empty, or the first entry is not the primary public entry point |
| WebDAV cannot connect | [WebDAV](/en/config/webdav/) / [Reverse Proxy](/en/deployment/reverse-proxy/#do-not-miss-these-when-proxying-webdav) | Proxy does not pass WebDAV methods, path prefix, or upload limit correctly |
| Office files will not open | [File Editing](/en/guide/editing/) / [System Settings: preview apps](/en/config/runtime/#site-configuration) | WOPI service cannot call back to AsterDrive; public site URL or CORS is wrong |
| Page looks broken after an upgrade | [Frontend Asset Cache](/en/deployment/frontend-assets/) | Browser, CDN, or proxy cached old assets |

## Configuration and Maintenance

| Symptom | Read first | Common causes |
| --- | --- | --- |
| You do not know whether to use the admin UI or edit files | [Configuration Overview](/en/config/) | Startup configuration, system settings, storage policies, and reverse proxy settings are mixed together |
| You cannot enter the admin UI but need to change configuration | [Operations CLI: config](/en/deployment/ops-cli/#offline-system-settings-config) | You need to view, validate, or write system settings offline |
| You are preparing to upgrade and worry about rollback | [Upgrades and Version Migration](/en/deployment/upgrade/) / [Backup and Restore](/en/deployment/backup/) | Old binary/image, configuration, database, and upload-directory backups are not prepared |
| Terminology is unclear | [Glossary](/en/reference/glossary/) | First separate primary node, follower node, storage policy, policy group, and remote storage target |

## Still Not Solved?

Collect these details before opening an issue or asking someone to look:

- AsterDrive version
- Deployment method: Docker, systemd, or direct binary run
- Database backend: SQLite, PostgreSQL, or MySQL
- Storage policy type and backend configuration
- Reverse proxy type and key configuration
- Browser console error, server logs, and the related error code

If you have an error code, read [Error Code Handling](/en/reference/errors/) first. If there is no error code but the symptom is clear, start from [Troubleshooting](/en/deployment/troubleshooting/).
