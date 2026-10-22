# Deployment

## Choose a deployment

Both container options work in an empty directory using published amd64/arm64 images. No repository checkout or build toolchain is required.

| You have | Use | Files to keep |
|---|---|---|
| An existing Garage cluster | [UI only](#connect-to-an-existing-garage) | `compose.yaml` and a private `.env` |
| No Garage installation | [Garage + UI](#run-garage-and-the-ui-together) | `compose.yaml` |

For the shortest instructions, use the [README quick start](https://github.com/xjoker/garage-ui/blob/dev/README.md#quick-start-docker-compose). This page covers network connections, HTTPS, other installation methods, and maintenance.

## Connect to an existing Garage

Create a deployment directory and save this as `.env`, replacing the example values:

```dotenv
GARAGE_ADMIN_URL=http://192.0.2.10:3903
GARAGE_ADMIN_TOKEN='replace-with-your-garage-admin-token'
S3_ENDPOINT_URL=http://192.0.2.10:3900
S3_REGION=garage
COOKIE_SECURE=false
```

Use the `admin_token` from Garage's configuration or an appropriately scoped admin token. An S3 access key is a separate credential and cannot authenticate the Admin API. The region must match Garage's `[s3_api].s3_region`.

Use the [UI-only Compose configuration](https://github.com/xjoker/garage-ui/blob/dev/deploy/compose.external.yaml), saved as `compose.yaml` beside `.env`:

```sh
chmod 600 .env
docker compose up -d
```

Open [http://localhost:3909](http://localhost:3909) and create the first UI administrator. See [Getting started](Getting-started.md) to create a bucket, grant an S3 key access, and upload a file.

The Compose file pins a UI release, restarts it unless explicitly stopped, and persists UI state in a named volume. The UI runs as UID/GID 65532 with a read-only root filesystem; a one-shot init container gives it ownership of the volume. No external database is needed. Set `GARAGE_UI_VERSION` in `.env` to select another published release.

### Reach your existing Garage

Both the Admin and S3 URLs must work **from the UI container**. They are backend connections, so the browser does not need access to the Admin API.

| Where Garage runs | Addresses to use |
|---|---|
| Another machine | Its private IP or DNS name, such as `http://192.0.2.10:3903` and `http://192.0.2.10:3900` |
| A container on the same Docker network | Its service name, such as `http://garage:3903` and `http://garage:3900` |
| The Docker host | `http://host.docker.internal:3903` and `http://host.docker.internal:3900`, subject to the host setup below |

To join Garage's existing Docker network, create `compose.override.yaml` beside `compose.yaml`, replacing the network name with the actual one shown by `docker network ls`:

```yaml
networks:
  default:
    external: true
    name: your-existing-garage-network
```

Compose loads this override automatically. Use Garage's service name or network alias in `.env`; sharing a physical host alone does not connect separate Compose networks.

Docker Desktop supplies `host.docker.internal`. On Linux Docker Engine, add this service override to `compose.override.yaml` when using that hostname:

```yaml
services:
  garage-ui:
    extra_hosts:
      - "host.docker.internal:host-gateway"
```

Garage must listen on an address reachable from that container network. A host service bound only to `127.0.0.1` is not reachable through the Linux bridge gateway. Keep the Admin API restricted to the private network; it does not need a public port.

After changing `.env` or an override, run `docker compose up -d` to recreate the affected service. `docker compose restart` does not apply changed environment variables.

## Run Garage and the UI together

Deploy with the [Garage + UI Compose configuration](https://github.com/xjoker/garage-ui/blob/dev/deploy/compose.yaml), saved as `compose.yaml`:

```sh
docker compose up -d
```

The configuration uses published UI and Garage v2.4.1 images. Initialization is included in the Compose file; no extra script or `.env` is required.

The stack generates separate RPC, Admin, and metrics secrets, preserves them on subsequent starts, and starts Garage with `--single-node` to initialize its layout. Only Admin/metrics tokens are mounted into the UI; its container cannot read Garage's RPC secret.

| Volume | Contents |
|---|---|
| `garage-ui-data` | UI accounts, sessions, audit, bucket-key selections, and session key |
| `garage-data` | Garage objects and metadata |
| `garage-config` | Generated Garage configuration and credentials |
| `garage-ui-secrets` | Admin/metrics tokens readable by the UI |

Compose prefixes these names with the project name. Keep the deployment directory name or an explicit `-p` project name consistent when recreating the stack, so it reuses the same volumes.

Open [http://localhost:3909](http://localhost:3909) and create a UI administrator. This setup provides **one Garage node without data redundancy**; use Garage's [cluster deployment guide](https://garagehq.deuxfleurs.fr/documentation/cookbook/real-world/) for production storage.

By default, the UI and Garage ports 3900–3903 are published only on loopback. The UI connects to Garage internally, using `http://garage:3903`, `http://garage:3900`, and region `garage`.

Optional settings go in a `.env` beside `compose.yaml`:

```dotenv
PORT=3909
COOKIE_SECURE=false
GARAGE_UI_VERSION=20261022.1
# Set only when recipients can reach S3 at this address:
# S3_PUBLIC_ENDPOINT_URL=https://s3.example.com
```

Setting a public S3 URL does not expose the S3 service. Configure its own reachable address or HTTPS reverse proxy before creating share links; the default loopback binding cannot be reached by recipients on another machine. Garage Admin/S3 connection settings for this stack are fixed in `compose.yaml`; the four connection variables for the UI-only deployment do not override them.

## Serve the UI over HTTPS

For remote setup over SSH, keep the default loopback binding and local HTTP settings:

```sh
ssh -L 3909:127.0.0.1:3909 your-server
```

Replace `your-server` with your SSH destination and open [http://localhost:3909](http://localhost:3909) on your workstation.

For ongoing remote access, put an HTTPS reverse proxy in front of `127.0.0.1:3909`. In the minimal README examples, change the UI service’s `COOKIE_SECURE` environment value to `"true"`; the extended deployment files read `COOKIE_SECURE=true` from `.env`. Then run `docker compose up -d`. A proxy in another container needs a shared Docker network and must reach `garage-ui:3909`; its loopback address is not the host's.

If the proxy overwrites `X-Forwarded-For` and direct access is restricted, add this to `compose.override.yaml` so login limits and audit records use the correct client address:

```yaml
services:
  garage-ui:
    environment:
      TRUSTED_PROXY: "true"
```

Only enable this setting for a trusted proxy. See [Configuration](Configuration.md) for `BASE_PATH`, secret files, private CAs, and other settings. Adding a new application variable to Compose's `.env` does not pass it into the container unless the Compose file references it; use a service `environment` override for settings absent from the supplied file.

## Check startup, restart, or stop

```sh
docker compose ps
docker compose logs --tail=100 garage-ui
curl --fail --silent http://127.0.0.1:3909/healthz
docker compose down
```

Use `docker compose restart garage-ui` to restart without changing configuration. Use `docker compose up -d` after editing settings.

`down` preserves volumes. **Adding `--volumes` deletes their data**, including stored objects in the combined stack. To stop only the UI while leaving Garage running, use `docker compose stop garage-ui`.

If an image pull reports `unauthorized`, check that the chosen release exists and its GHCR package is public; public release images need no GitHub account or Docker login. A public source repository alone does not make its container package public. For login loops or connection failures, use [Troubleshooting](Troubleshooting.md).

## Docker without Compose

Create a private `.env` with the four Garage connection settings and `COOKIE_SECURE=false` as in [UI-only deployment](#connect-to-an-existing-garage), then run:

```sh
docker run -d --name garage-ui --restart unless-stopped \
  --read-only --tmpfs /tmp --env-file .env \
  -p 127.0.0.1:3909:3909 -v garage-ui-data:/data \
  ghcr.io/xjoker/garage-ui:20261022.1
```

Open [http://localhost:3909](http://localhost:3909). For HTTPS use `COOKIE_SECURE=true`. Add `--network` with Garage's network name when connecting to another container. On Linux, add `--add-host host.docker.internal:host-gateway` to reach a host service by that name.

The image gives a fresh named volume the non-root user's ownership; an existing directory or volume must already be writable by UID/GID 65532. Keep `/data` mounted across container replacements.

## Download the Linux binary

Choose `garage-ui-linux-amd64` for an x86-64 Linux host or `garage-ui-linux-arm64` for an ARM64 Linux host. These binaries do not run natively on macOS or Windows; use Docker there. Download the binary and `SHA256SUMS` from the same [release](https://github.com/xjoker/garage-ui/releases), then verify. `SHA256SUMS` lists every release file under its download name, including `default.env.example`; keep that name while verifying and rename it to `.env` afterwards. `--ignore-missing` only skips files you did not download:

```sh
sha256sum --check --ignore-missing SHA256SUMS
chmod +x garage-ui-linux-amd64
```

Set the four Garage connection variables in the process environment. For example, with a local Garage and a token stored in a private file:

```sh
export GARAGE_ADMIN_URL=http://127.0.0.1:3903
export API_ADMIN_KEY_FILE="$HOME/.config/garage-ui/admin-token"
export S3_ENDPOINT_URL=http://127.0.0.1:3900
export S3_REGION=garage
export COOKIE_SECURE=false
./garage-ui-linux-amd64
```

Replace the addresses and region with your Garage configuration, create the token file before starting, and use the ARM64 filename on that architecture. The binary defaults to `127.0.0.1:3909`; set `PORT` to use another port, for example to run a second instance with its own `DATA_DIR`. It stores UI state in `./data`, which it keeps at mode `0700`. Preserve that directory across upgrades. Binaries and images are unsigned; SHA-256 checksums verify integrity, not publisher identity. Releases also include an SPDX SBOM.

## Build from source

Source builds are for contributors or custom changes. Follow [Contributing](https://github.com/xjoker/garage-ui/blob/dev/CONTRIBUTING.md#set-up-local-development) for Rust/Bun prerequisites and `scripts/build.sh`.

Use `scripts/build.sh --images amd64` or `arm64` to load a local container image; omit the architecture to build both. The default local tag is `garage-ui:<VERSION>-local-<architecture>`. These builds are not pushed.

Both Compose files use published images and have no source build context. To use a custom image, tag it as `ghcr.io/xjoker/garage-ui:local`, set `GARAGE_UI_VERSION=local` in `.env`, and start with `docker compose up -d --pull never`.

## Verify the deployed version

```sh
curl --fail --silent http://127.0.0.1:3909/healthz
```

The response includes `version`, `commit`, `status`, and `build_time`. Compare the version and full commit with the release tag. Probe the actual deployment address when verifying a remote instance; a successful push or a running container does not prove that the intended image is in use. **Settings → About** also shows the version.

## Deploy the UI with Helm

The [Helm chart](https://github.com/xjoker/garage-ui/tree/dev/deploy/helm/garage-ui) installs the UI only and expects an existing Garage cluster. Configure `garageAdminUrl`, `s3EndpointUrl`, and `s3Region` in a private values file. Supply `secrets.garageAdminToken` through private deployment configuration. Set `s3PublicEndpointUrl` when sharing is needed. Do not commit secrets.

```sh
helm upgrade --install garage-ui deploy/helm/garage-ui -f private-values.yaml
```

The chart uses the release image by default. Override `image.repository` and `image.tag` for another image, and `persistence.size` for the UI volume. The volume is a PersistentVolumeClaim from the cluster's default StorageClass. The UI refuses to start on a volume that other users can write to, which includes the `0777` volumes that local-path (the k3s default) and NFS subdirectory provisioners create; see [Troubleshooting: Kubernetes volumes](Troubleshooting.md#kubernetes-volumes) before installing on such storage. `trustedProxy: true` is appropriate only behind an ingress or proxy that overwrites `X-Forwarded-For`. The TLS settings `tls.caSecret` and `tls.insecure` apply to outgoing Garage connections, not browser HTTPS; arrange ingress TLS separately. Liveness uses the binary's health check, while readiness uses `/readyz`.

## Back up and restore UI state

Preserve **all of `DATA_DIR`**, including `garage-ui.sqlite`, any SQLite WAL/SHM sidecars, and `session.key`. If using `SESSION_KEY_FILE`, preserve that external key separately. A simple consistent filesystem backup is to stop the UI service before copying or snapshotting its volume, then restart it after the backup. Do not copy only the main SQLite file while writes continue.

The UI keeps `DATA_DIR` at mode `0700` and its files at `0600`, owned by the service user (UID 65532 in the container image). Run backup and monitoring jobs that read it as that user or as root; a job running as another user cannot read the directory. A backup holds password hashes, sessions, and audit records: store it as sensitively as the live volume.

To restore, stop the UI, restore the complete saved state and key, ensure the runtime user owns the data directory and can read/write it, and restart with the intended configuration. Startup changes restored files back to `0600`, and stops with an error if other users, or a group other than the runtime user's own, can write to the directory. Verify that the expected users and audit records appear and Garage connections work. Keep the saved copy until restoration is verified.

UI backups preserve local accounts, sessions, audit, ZIP tokens, and bucket-key selections. Objects, Garage keys/tokens, cluster metadata, and Garage snapshots require Garage's own backup process.

Backups taken before 20261019.1 store session cookies in plain text. Restoring such a backup with a version before 20261019.1 makes those cookies valid again until they expire (24 hours at most by default); 20261019.1 and later delete them at the first start. After a rollback to a version before 20261019.1 and a later upgrade, the sessions the older version created are not deleted at startup: they cannot sign in to the newer version, and they are removed when they expire.

## Upgrade deliberately

Back up UI state first. In the minimal README examples, change the UI service’s `image` tag to the chosen release. The extended deployment files select it through `GARAGE_UI_VERSION` in `.env`. Then pull and recreate the service with the same volume and configuration:

```sh
docker compose pull garage-ui
docker compose up -d --no-deps garage-ui
```

Keep the saved `COOKIE_SECURE` setting appropriate for HTTP or HTTPS. These commands update only the UI; upgrade Garage separately using its own upgrade guide. Check **Settings → About**, verify `/healthz`, and confirm login and a storage operation. Keep your backup until verification succeeds.

Upgrading from a version before 20261019.1 signs every user out once: the first start deletes the sessions an earlier version stored, and the log records `signed out sessions stored before session digests`. Sign in again; scripts that hold a session cookie need a new one. Accounts, passwords, and roles are unchanged.

## How release artifacts are produced

The release workflow is currently disabled on GitHub. Its configured trigger is version tags, following the [release policy](https://github.com/xjoker/garage-ui/blob/dev/docs/engineering.md#79-release-and-images). Ordinary branch pushes and pull requests are not configured to build. Maintainers run local verification and the release scripts while the workflow is disabled.

A valid `YYYYMMDD.N` tag must match `VERSION` and the package manifests. When enabled, the workflow validates this match before building and verifying the amd64/arm64 images and static binaries, generating an SBOM and checksums, and publishing the images and GitHub release attachments. `compose.yaml` and `default.env.example` (to save locally as `.env`) are included so users can deploy without cloning the repository.

GHCR visibility is independent of repository visibility. The maintainer must make the package public once; the publication process verifies anonymous access before publishing the release. Images and binaries are unsigned, with signing and build-provenance attestations deferred.
