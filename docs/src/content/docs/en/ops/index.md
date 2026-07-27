---
description: "AsterDrive operations section home, organized by launch lifecycle covering first-start checks, production launch acceptance, monitoring, capacity, backup, upgrade, troubleshooting, and the operations CLI."
title: "Operations Overview"
---

:::tip[What this section covers]
Deployment answers "how to get it running". This section answers **what comes after**: what to validate after startup, what to check before launch, which metrics to watch, how to back up and upgrade, and where to look when something breaks.
:::

## Follow the Lifecycle

| Stage | Page |
| --- | --- |
| Right after first startup | [First-Start Checklist](/en/ops/first-check/) |
| Before production launch | [Production Launch Checklist](/en/ops/launch-checklist/) |
| Day-to-day observation | [Monitoring and Grafana](/en/ops/monitoring/), [Capacity Planning](/en/ops/capacity/) |
| Data safety | [Backup and Restore](/en/ops/backup/) |
| Version changes | [Upgrade and Version Migration](/en/ops/upgrade/) |
| Something is broken | [Troubleshooting](/en/ops/troubleshooting/) |
| Command-line work | [Operations CLI](/en/ops/cli/): doctor, offline configuration, node enroll, cross-database migration |

## When to Load Test

When changing storage backends, tuning background task concurrency, or evaluating version regressions, run the repository's built-in k6 benchmarks in your own environment: [Performance Benchmarking and Load Testing](/en/ops/capacity/benchmarking/). The measured inputs for capacity estimation (p95, throughput, memory footprint) also come from there.
