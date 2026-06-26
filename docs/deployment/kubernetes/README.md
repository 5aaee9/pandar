# Pandar Helm Chart

This chart deploys `pandar-hub` and `pandar-web` on Kubernetes.

The published OCI chart is stored under:

```sh
oci://ghcr.io/5aaee9/pandar/chart/pandar
```

Install the main pre-release chart:

```sh
helm install pandar oci://ghcr.io/5aaee9/pandar/chart/pandar --version 0.1.0-main
```

Install a release chart:

```sh
helm install pandar oci://ghcr.io/5aaee9/pandar/chart/pandar --version 0.1.1
```

The default values run a single Hub replica with SQLite at `/data/pandar.db` and filesystem artifacts under `/spool`, both backed by PVCs. For production PostgreSQL, provide `PANDAR_DATABASE_URL` through an existing Secret:

```sh
kubectl create secret generic pandar-database \
  --from-literal=PANDAR_DATABASE_URL='postgres://pandar:password@postgres:5432/pandar'

helm upgrade --install pandar oci://ghcr.io/5aaee9/pandar/chart/pandar \
  --version 0.1.1 \
  --set hub.database.existingSecret=pandar-database
```

If the API is exposed outside the cluster, set `web.appApiUrl` to the public Hub URL so browser WebSocket connections use the same external origin:

```sh
helm upgrade --install pandar oci://ghcr.io/5aaee9/pandar/chart/pandar \
  --version 0.1.1 \
  --set web.appApiUrl=https://api.example.com
```

When `hub.image.tag` and `web.image.tag` are empty, the chart uses the packaged `appVersion` as the image tag. Main-branch packages use `appVersion: main`; release packages use the Git tag, for example `v0.1.1`.

Main-branch CI packages the chart as `<Chart.yaml version>-main`, falling back to `0.1.0-main` only if no chart version can be read. Tag CI packages release charts from the tag, for example `v0.1.1` publishes chart version `0.1.1`.
