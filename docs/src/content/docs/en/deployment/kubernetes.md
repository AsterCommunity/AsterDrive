---
title: "Kubernetes Deployment"
description: Deploy multiple AsterDrive Primaries with a StatefulSet and correctly configure shared databases, Redis, storage, stable internal endpoints, health probes, and shared avatars.
---

The repository provides both Kustomize and Helm deployments for multiple AsterDrive Primaries. Both assume the cluster contract documented in [Load Balancing and Multi-Instance Deployments](/en/deployment/load-balancing/) and **manage only AsterDrive itself**. They do not take ownership of production PostgreSQL/MySQL, Redis, object storage, an Ingress controller, or a certificate controller.

These dependencies are intentionally not default subcharts because the database, Redis, and object storage hold authoritative state. Coupling them to the application chart would make application upgrades, rollbacks, and uninstall operations affect data services, while failing to represent managed databases, cloud object storage, existing Redis clusters, and environment-specific backup policies. The OrbStack overlay includes these dependencies only as local smoke and failure-injection fixtures.

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

## Production Kustomize example

`deploy/kubernetes/overlays/production-example/` reuses the shared base and produces a deployment with:

- a pinned image-version example
- `automountServiceAccountToken: false`
- Restricted Pod Security Namespace labels
- CPU and memory limits
- `DoNotSchedule` node topology spreading
- an NGINX Ingress starting point
- an ingress-only NetworkPolicy

Before applying it, replace the RWX StorageClass, image version or digest, host, TLS Secret, and IngressClass. The NetworkPolicy permits traffic within the AsterDrive Namespace and from Namespaces labeled `asterdrive.io/ingress-access=true`; label the actual Ingress controller Namespace or edit the policy to match your traffic entry point.

The policy intentionally leaves egress unrestricted because a generic template does not know the real addresses of DNS, the database, Redis, object storage, OAuth/OIDC, SMTP, or remote Followers. Enumerate every dependency before adding default-deny egress, otherwise readiness, login, email, remote storage, or download flows can be blocked by the deployment itself.

```bash
kubectl label namespace ingress-nginx asterdrive.io/ingress-access=true
kubectl kustomize deploy/kubernetes/overlays/production-example
kubectl apply --dry-run=server -k deploy/kubernetes/overlays/production-example
kubectl apply -k deploy/kubernetes/overlays/production-example
```

## Helm

The chart is located at `deploy/helm/asterdrive/`. It uses the same cluster defaults, probes, stable internal endpoints, security contexts, temporary volumes, and RWX avatar-volume contract as the Kustomize base.

```bash
helm upgrade --install asterdrive deploy/helm/asterdrive \
  --namespace asterdrive \
  --create-namespace \
  --set image.digest=sha256:REPLACE_WITH_IMAGE_DIGEST \
  --set avatarPersistence.storageClass=REPLACE_WITH_RWX_STORAGE_CLASS
```

The chart references the `asterdrive-cluster` Secret by default. Database URLs, Redis endpoints, and application keys stay out of ordinary Helm values; create the Secret through the cluster's existing secret-management system and set `existingSecret` when using a different name. The chart supports creating or reusing the avatar PVC, Ingress, ingress NetworkPolicy, image tags or digests, resources, scheduling, extra environment variables, and PDB settings. It does not include PostgreSQL, Redis, or object-storage subcharts.

For multi-node production clusters, set `topologySpread.whenUnsatisfiable=DoNotSchedule`. The default remains `ScheduleAnyway` for development and single-node validation. With the strict setting, a Pod stays Pending when there are not enough failure domains; that is the intended protection and should be resolved by adding capacity rather than colocating both Primaries.

## Local validation

You can render and strictly validate the manifests with the same kubeconform image as CI without contacting a workload cluster:

```bash
kubectl version --client
kubectl kustomize deploy/kubernetes > /tmp/asterdrive-base.yaml
kubectl kustomize deploy/kubernetes/overlays/production-example > /tmp/asterdrive-production.yaml
helm lint deploy/helm/asterdrive
helm template asterdrive deploy/helm/asterdrive --namespace asterdrive > /tmp/asterdrive-helm.yaml
docker run --rm -v /tmp:/manifests \
  ghcr.io/yannh/kubeconform:v0.7.0@sha256:85dbef6b4b312b99133decc9c6fc9495e9fc5f92293d4ff3b7e1b30f5611823c \
  -strict -summary \
  /manifests/asterdrive-base.yaml \
  /manifests/asterdrive-production.yaml \
  /manifests/asterdrive-helm.yaml
```

`kubectl apply --dry-run=client` may still perform API discovery even when validation is disabled, so it is not a reliable offline validator for CI without a kubeconfig. CI renders the OrbStack and production overlays plus Helm boundary cases for default values, Ingress with NetworkPolicy and digest, and an existing PVC with three replicas, then runs strict schema validation with a pinned kubeconform image. These checks do not prove that the external database, Redis, RWX StorageClass, Ingress, or shared storage is operational. Before applying changes, connect to the target cluster and run `kubectl apply --dry-run=server`; after deployment, complete the [multi-instance launch validation](/en/deployment/load-balancing/#launch-validation).
