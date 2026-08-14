# tpt-daemon Helm chart

Deploys the [`tpt-daemon`](https://github.com/tpt-solutions/tpt-telemetry)
unified syslog → parse → OTLP exporter on Kubernetes.

## Install

```bash
helm install tpt-daemon ./deploy/helm/tpt-daemon \
  --namespace telemetry --create-namespace \
  --set config.otlp.endpoint=http://otel-collector:4318 \
  --set secretEnv.OTLP_TOKEN=changeme
```

## What it deploys

| Resource | Notes |
|----------|-------|
| `Deployment` | 1 replica by default; `NET_BIND_SERVICE` cap, non-root, read-only rootfs, distroless image. |
| `Service` | `ClusterIP` exposing syslog UDP/TCP (514) + metrics (9464). |
| `ConfigMap` (x2) | Rendered `tpt-daemon.toml` + the mounted `.tpt-log` schema. |
| `Secret` | Auto-created from `secretEnv` (or reference `existingSecret`). |
| `NetworkPolicy` | Restricts the unauthenticated `/metrics` + `/healthz` port to allowed scrapeers. |
| `PodDisruptionBudget` / `HPA` | Optional, gated by values. |

## Security notes

- The daemon binds privileged ports (514), so the pod is granted
  `NET_BIND_SERVICE` only; all other capabilities are dropped and the root
  filesystem is read-only (mirrors `packaging/tpt-daemon.service`).
- The `/metrics` and `/healthz` endpoints are **unauthenticated**. By default the
  chart binds them to loopback inside the pod and the `NetworkPolicy` limits
  ingress on the metrics port to pods matching
  `networkPolicy.metrics.allowedPodSelector` (Prometheus by default). Keep
  `service.type` as `ClusterIP` and never expose metrics via a public
  LoadBalancer.
- OTLP auth tokens are injected from `secretEnv`/`existingSecret` and
  interpolated into the config at runtime via `${ENV_VAR}` (the daemon's own
  mechanism), so they never land in the ConfigMap.

## Values

See [`values.yaml`](./values.yaml). Key overrides:

| Value | Default | Description |
|-------|---------|-------------|
| `replicaCount` | `1` | Daemon replicas (stateful ring buffer; scale mindfully). |
| `image.tag` | chart appVersion | Image tag. |
| `config.otlp.endpoint` | `http://otel-collector:4318` | OTLP collector. |
| `config.otlp.transport` | `http` | `http` or `grpc`. |
| `config.schema.path` | `/etc/tpt-daemon/schema.tpt-log` | Mounted schema path. |
| `schema.content` | Cisco ASA example | Override with your `.tpt-log` schema. |
| `secretEnv` | `{}` | Secret literals → `Secret` → container env. |
| `networkPolicy.enabled` | `true` | Restrict metrics ingress. |
