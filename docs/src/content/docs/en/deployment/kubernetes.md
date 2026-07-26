---
title: "Kubernetes Deployment"
description: Deploy multiple AsterDrive Primaries with a StatefulSet and correctly configure shared databases, Redis, storage, stable internal endpoints, health probes, and shared avatars.
---

The repository provides a `deploy/kubernetes/` example that runs two AsterDrive Primaries. It assumes the cluster contract documented in [Load Balancing and Multi-Instance Deployments](/en/deployment/load-balancing/) and does not create production PostgreSQL/MySQL, Redis, object storage, or an Ingress controller for you.

## Why a StatefulSet

Normal AsterDrive HTTP requests do not depend on Pod identity, but reverse-tunnel owner routing requires a unique and stable `deployment.internal_endpoint` for every Primary. The example combines StatefulSet Pod names with a headless Service:

```text
http://asterdrive-0.asterdrive-headless.asterdrive.svc.cluster.local:3000
http://asterdrive-1.asterdrive-headless.asterdrive.svc.cluster.local:3000
```

The StatefulSet is used only for stable DNS. Authoritative state remains in the shared database, Redis, and storage data plane; application correctness must not depend on Pod-local disks.

## Prerequisites

1. One PostgreSQL or MySQL database shared by every Primary; cluster mode rejects SQLite.
2. One Redis service used by both cache and config sync.
3. A storage policy reachable by every Primary, such as S3-compatible storage, Azure Blob, OneDrive, or a remote Follower; cluster mode rejects local policies.
4. A `ReadWriteMany` PVC for the default `/data/avatar` directory. Remove this mount only when uploaded avatars are disabled.
5. Authentication and encryption secrets that are identical on every Primary.
6. A per-Primary internal endpoint reachable only from the trusted cluster network, plus a shared internal proxy secret of at least 32 characters.

The required Secret keys, rendering commands, and manifest boundaries are listed in `deploy/kubernetes/README.md`. `secret.example.yaml` documents the fields but is excluded from kustomization; create the Secret through your existing secret-management workflow.

## Probes and termination

The example uses `/health` for startup and liveness probes, and `/health/ready` for readiness. Readiness always checks the shared database, the real Redis cache, and cluster topology. After product setup is complete, it also runs the default storage driver's lightweight readiness check. Pods that fail readiness are removed from the public Service.

A fresh cluster database has no default storage policy. As long as the database, Redis, and topology are healthy, `/health/ready` still returns `200` with a status of `needs_admin` or `needs_storage`, allowing the Pod to enter the public Service so setup can finish. Incomplete product setup is not a Pod failure. Create the first administrator and sign in with it; the frontend directs that administrator to the storage-policy page. After the administrator creates a shared storage policy reachable by every Primary and marks it as default, AsterDrive atomically creates the default policy group, assigns administrators that do not yet have a policy group, notifies the other Primaries through Redis to reload, and changes the status to `ready`. Normal registration, invitation acceptance, and administrator-created users receive an explicit setup-incomplete error during `needs_storage`.

Once the system reaches `ready`, `/health/ready` checks that the default policy exists, its driver can be constructed, and any local low-cost prerequisites exposed by that driver are satisfied. This high-frequency probe does not read from or write to S3, OneDrive, SFTP, or a remote Follower, so an external storage outage does not necessarily produce `503`; production deployments still need metrics alerts or a separate synthetic object-I/O probe. Redis, database, or topology failures return `503` during every setup state.

Before termination, the Pod waits for a 10-second `preStop` window so endpoints can be drained. The total `terminationGracePeriodSeconds` is 45 seconds. Existing SSE, upload, download, WebDAV, or reverse-tunnel connections may still be interrupted and must reconnect or retry.

## Volumes

Each Pod receives its own `emptyDir` at `/data`, while an RWX PVC is mounted over `/data/avatar`. This prevents multiple processes from writing the same generated config and temporary directories while keeping uploaded avatars readable across instances.

Do not use a shared PVC to bypass local-policy or Pod-local staging restrictions. Cluster mode rejects those paths according to driver and upload strategy; shared object storage or connector-native multipart remains the supported multi-Primary data plane.

## Ingress

`ingress.example.yaml` is an NGINX Ingress example and is not part of kustomization. Replace its host, TLS Secret, and IngressClass, then verify that the controller provides:

- unbuffered streaming for SSE, downloads, uploads, and WebDAV
- WebSocket Upgrade for reverse tunnels
- a request body limit large enough for the intended uploads
- sufficiently long read, write, and idle timeouts
- correct `Host`, public-scheme, and trusted-proxy forwarding
- cluster-wide rate limiting at the edge when strict global counters are required

## Local validation

You can validate rendering before touching a workload cluster:

```bash
kubectl version --client
kubectl kustomize deploy/kubernetes
kubectl apply --dry-run=client -k deploy/kubernetes
```

Client-side dry-run validates the local manifests and schemas known to kubectl. It does not prove that the external database, Redis, RWX StorageClass, Ingress, or shared storage is operational. After deployment, complete the [multi-instance launch validation](/en/deployment/load-balancing/#launch-validation).
