---
description: "The Runtime group of System Settings: background task cadence, lane concurrency, task artifact retention, share streaming playback session TTL, and list limits."
title: "Runtime"
---

Entry point: `Admin -> System Settings -> Runtime`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

Administrators decide the pace of background work here. Default behavior:

| Task | Default Frequency |
| --- | --- |
| Mail queue scan | Every 5 seconds |
| Background task queue scan | Every 5 seconds |
| Background task idle backoff maximum | Every 60 seconds |
| Periodic cleanup | Every 1 hour |
| Low-level file consistency check | Every 6 hours |
| System health checks (database / cache / follower nodes) | Every 5 minutes |

You can also tune:

- Background task idle backoff maximum
- Temporary background task artifact retention
- Reserved fallback background task concurrency limit. It is currently used only by future unclassified task kinds, not by existing task lanes
- Concurrency limit for archive tasks: online compression, online extraction, and archive preview
- Thumbnail generation task concurrency limit
- Storage policy migration task concurrency limit
- Maximum background task attempts
- Share download rollback queue capacity
- Share streaming playback session TTL
- System health check interval
- Team member list page size limit
- Task list page size limit

If there are no obvious performance issues, queue backlogs, or follower node detection delays, keep the defaults.  
If you increase concurrency for the archive, thumbnail, or storage-migration lanes, matching tasks can run together more easily, and CPU, memory, network, and I/O pressure will increase with them. The reserved fallback concurrency cap is not a "global total concurrency" setting, so do not rely on it to limit every background task.

`Task retention` controls how long temporary background task artifacts are kept, defaulting to `24` hours. It mainly affects temporary files or downloadable results produced by package downloads, online compression, online extraction, and link-import tasks. Task records themselves remain as history in the task list until an administrator conditionally cleans finished records.

Audio and video on share pages create a short-lived streaming playback session first to support Range playback. The default TTL is `3` hours, configurable from `5` minutes to `24` hours. Longer TTLs work better for long background music playback; shorter TTLs reduce the access window after a link leak.
