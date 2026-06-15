---
description: Why AsterDrive exists, what it aims to become, who it fits, and why the author treats it as an open self-hosted file hub.
---

# About AsterDrive

::: tip This page does not teach operations
This is closer to an author's note.

If you only want to get the service running first, go to [Getting Started](./getting-started). If you want to know why this project exists, keep reading.
:::

## The First Alpha

The first alpha release of AsterDrive was really just a file browser.

It could upload, download, and recover files from the trash after a mistaken deletion. At that time I was not trying to build a complete cloud suite, and I had not drawn a grand architecture diagram first. I just wanted a place that felt right for my own use. Files could go up, come back down, and be recovered after accidental deletion. Those three things.

It sounds small, but to me it was already enough reason to start a project.

The original motivation was not grand either. I had simply used existing self-hosted cloud drives and felt they did not fit my hands. Some capabilities I needed were behind commercial editions; some interactions I touched every day never felt the way I wanted; large-file uploads occasionally stuck, though I could not always tell whether the software itself was at fault or whether my deployment was wrong.

The feeling was clear: it worked, but I could not treat it as the place I wanted to rely on long term.

So I started writing my own.

## Giving Files a Place to Stop

If I could keep only three features today, I would still choose upload, download, and trash.

Upload and download are the entrance. Trash is trust. Without trash, I would not really feel safe putting important files inside. People misclick, scripts are written wrong, and action menus can be clicked by accident. If a file system gives me no room to regret an action, it is hard for it to become something I truly depend on.

So the first thing AsterDrive needed to do was plain: give files a place to stop.

Later, I began thinking of it as a hub.

First, this hub has to let files dock safely. After files come in, there are directories, permissions, trash, versions, locks, and background cleanup. When something goes wrong, you should be able to see what happened, trace logs, run checks, and recover what can be recovered.

Then it needs to route flows. Files do not have to stay on this machine forever. They may land on different storage policy backends, including object storage, Microsoft Graph-backed drives, or another AsterDrive node. Browsers, WebDAV clients, Office editors, share pages, and background tasks can all be entry points. AsterDrive's job is to make these entry points, destinations, permissions, and lifecycles explicit and understandable.

Finally, it should let me stay in control as much as possible. Where files are, who can access them, whether they can be shared, how long they remain after deletion, and which upload path large files take are not things I want to hand entirely to a black box.

## Leaving Room Early

On the surface, the first alpha was only a file browser. But before alpha.1, I had already started thinking about the storage layer that would come later.

I knew early that it could not forever treat files as paths under a local directory. Files would someday leave this machine, go to object storage, go to another node, or go somewhere I had not fully figured out yet. So while it was still small, I started leaving room.

This is why AsterDrive later grew storage policies, policy groups, S3, remote follower nodes, and large-file upload negotiation. They all answer the same question:

> After a file comes in, how should it reliably reach where it belongs?

Today AsterDrive can start from default SQLite + local storage, and it can also connect to PostgreSQL / MySQL, S3-compatible object storage, Tencent COS, and remote follower nodes. It can handle regular direct uploads, resumable chunked uploads, object-storage presigned uploads, and multipart uploads. It no longer sounds like the original file browser, but it still grew from those same three things: upload, download, and trash.

## What I Want It to Become

I want AsterDrive to be usable first.

It should start, upload, download, delete, restore, upgrade, and be diagnosable when errors happen. For file-system-like software, the most basic requirements are never romantic: data should not disappear casually, failures should not happen silently, and recovery paths should not exist only as wishes.

Then it should feel good to use.

By good to use, I do not mean pretty in a demo video. I mean smooth in real daily use. Opening a folder, dragging files in, creating a share, changing permissions, checking tasks, emptying trash, mounting WebDAV, and opening documents through an Office service should not force people to rethink the workflow every time.

Finally, it should fit my own use.

That may sound selfish, but I think a personal project can only grow real judgment if its author wants to rely on it long term. Fit for me means light, fast, changeable, able to start from one machine, and still leaving a path to expand later.

If other people happen to need the same thing, that is good. You can run it, fork it, and change it into something that fits you better.

## Why Rust

Short answer: because I already write Rust.

A more complete answer: Rust matches the delivery style I want. AsterDrive can be shipped as a single server program with embedded frontend assets, default SQLite, and no requirement to assemble a full runtime environment first. Memory safety, performance, and explicit error handling also fit file services, where boundary conditions are easy to hit.

Other languages can certainly write file services too. For this project, Rust happens to fit the "light, steady, and changeable" shape I want.

## Who It Fits

If you want to control your own files instead of handing everything to a commercial cloud drive, AsterDrive may fit you.

If you want to share photos, videos, documents, and materials with family, friends, or a small team, and you want uploads, sharing, recovery, preview, and integration with existing workflows through WebDAV or Office services, AsterDrive may fit you.

If you care where files ultimately land, want to start from local disk, and later move part of the data to S3 or follower nodes, AsterDrive may fit you.

If you want to build on top of a file service without being crushed at the start by a complex ecosystem, plugin marketplace, and heavy historical baggage, AsterDrive may also fit you.

Those repeated "may" phrases are intentional. File systems are close to everyone's data habits. Running it once is more honest than reading ten pages of introduction.

## Who It Does Not Fit

If you need calendars, contacts, chat, mail, a plugin marketplace, and a mature enterprise collaboration ecosystem today, AsterDrive is not currently for you.

If your most urgent need is a mature local folder two-way sync client, calm down first. I do plan to build clients for macOS, Windows, Linux, iOS, and Android as a long-term goal. But the first stage is better spent on access, upload, download, sharing, preview, and management. Real local two-way sync needs more caution because when sync goes wrong, the cost is much higher than on the web.

If you only want to wrap a web management UI around one directory on a server, AsterDrive may be too heavy.

If you need multi-primary hot standby, automatic failover, cross-region strong-consistency replication, complex enterprise permission matrices, or compliance certification, AsterDrive is also not suitable today. The current follower-node capability solves "write objects to another machine"; it is still far from a cluster orchestration system.

Admitting these boundaries matters. When a file system promises too much, user data is what gets hurt.

## Not Pretending to Be More Reliable Than Reality

I will not write phrases like "absolutely secure" or "100% reliable".

Disks fail, networks drop, disks fill up, and software has bugs. What AsterDrive can do is make upload, download, metadata, trash, versions, and background cleanup as solid as possible; provide checks such as `doctor`; emit logs and error codes that help locate problems; and document backup and restore clearly.

Backups remain the deployer's responsibility. You can start from [Backup and Restore](/en/deployment/backup), and you can also walk through the [Production Checklist](/en/deployment/production-checklist) before going live.

I want AsterDrive to make people feel safe. That safety should come from clear mechanisms, not just pretty slogans.

## The Worst Ending

Of course I hope people use AsterDrive.

If one day it can really support me financially, that would be good. A project being needed by others and allowing its author to keep investing in it is both realistic and precious.

But if the worst outcome is that nobody uses it, I still want it to remain a complete, open project that others can take and keep changing. The code is here. The license lets you read, modify, deploy, fork, and even grow a different direction from my ideas.

That is also why I tend to prefer permissive licenses such as MIT / Apache.

I do not want a project to count as alive only while it is in my hands. It began because my own tools did not fit my hands. If one day someone else runs into a similar mismatch, I hope they do not have to start from zero.

That is the most important meaning of AsterDrive to me.

It may not become the largest self-hosted file project.

But I will keep writing it into a place I am willing to trust with files.

## Want to Participate

- **Run it first**: start from [Getting Started](./getting-started).
- **Hit a problem**: describe the reproduction path clearly in [GitHub Issues](https://github.com/AptS-1547/AsterDrive/issues).
- **Want to change code**: fork, change, and open a PR. Changes driven by real problems are the most valuable.
- **Want to support the project**: you can [Sponsor](https://github.com/sponsors/AptS-1547), or help improve it through issues, documentation feedback, and deployment cases.
- **Commercial deployment / custom development**: contact the author from the GitHub homepage.

May it fit your hands.
