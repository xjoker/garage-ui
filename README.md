<p align="center">
  <img src="web/public/brand/garage-ui.svg" width="80" alt="Garage UI logo">
</p>

# Garage UI

[![Releases](https://img.shields.io/badge/Releases-GitHub-blue)](https://github.com/xjoker/garage-ui/releases)

**Manage your Garage cluster, buckets, and files in one browser.**

English | [简体中文](README.zh-CN.md)

An unofficial web interface for [Garage](https://garagehq.deuxfleurs.fr/), the S3-compatible object store. Browse and upload files, manage access, and check cluster health. One container includes the server and web interface; no separate database is required.

## Quick start: Docker Compose

Copy one of the examples below into `compose.yaml`. Use the published images directly; no repository checkout or build is needed.

### Connect to an existing Garage

Replace the Admin/S3 URLs, admin token, and region with your Garage settings:

```yaml
services:
  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261022.2
    restart: unless-stopped
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://192.0.2.10:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://192.0.2.10:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
    volumes:
      - garage-ui-data:/data
volumes:
  garage-ui-data:
```

Use addresses reachable from the UI container. Its `localhost` is not your Garage server. If you replace the named data volume with a host directory, add the `garage-ui-init` service shown below and mount that same directory into both UI services. The admin token is separate from an S3 access key. See [container networking](docs/wiki/Deployment.md#reach-your-existing-garage) if Garage runs on the same host or another Docker network.

### Run Garage and the UI together

Place your `garage.toml` beside `compose.yaml`, then use:

```yaml
x-logging: &default-logging
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"

services:
  garage:
    image: dxflrs/garage:v2.4.1
    restart: unless-stopped
    logging: *default-logging
    command: ["/garage", "server", "--single-node"]
    volumes:
      - ./garage.toml:/etc/garage.toml:ro
      - ./garage-data:/var/lib/garage
    networks:
      - garage

  garage-ui-init:
    image: alpine:3.22
    user: "0:0"
    network_mode: none
    command: ["sh", "-c", "chown 65532:65532 /data && chmod 0700 /data"]
    volumes:
      - ./garage-ui-data:/data

  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261022.2
    restart: unless-stopped
    logging: *default-logging
    depends_on:
      garage:
        condition: service_started
      garage-ui-init:
        condition: service_completed_successfully
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://garage:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://garage:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
      NO_PROXY: localhost,127.0.0.1,garage
    volumes:
      - ./garage-ui-data:/data
    networks:
      - garage

networks:
  garage:
    name: garage
```

The one-shot `garage-ui-init` service sets the UI data directory ownership automatically before the non-root UI starts. Both services must mount the same dedicated UI directory. `NO_PROXY` keeps internal Garage requests off inherited proxies. Set the UI's admin token and region to match `garage.toml`. Garage's metadata and data paths must be under `/var/lib/garage`, or adjust the volume mounts to match your configuration. For a new installation, prepare `garage.toml` using the [Garage quick start](https://garagehq.deuxfleurs.fr/documentation/quick-start/).

This example initializes a **new single-node Garage** without data redundancy. Use the previous UI-only example for an existing cluster. Garage's APIs stay inside the Docker network; publish S3 separately if other clients need access.

### Start and open the UI

```sh
docker compose up -d
```

Open **[http://localhost:3909](http://localhost:3909)** and create your first UI administrator. Existing buckets automatically use an available S3 key with read and write access. For a new bucket, create an S3 key, grant it access, and open **Objects** to upload a file.

The examples use local HTTP. For remote access, configure [HTTPS](docs/wiki/Deployment.md#serve-the-ui-over-https) and change `COOKIE_SECURE` to `"true"`. For a remote Docker host, use `ssh -L 3909:127.0.0.1:3909 your-server` to access the local port.

## Manage the deployment

Run these commands in the directory containing `compose.yaml`:

```sh
docker compose ps
docker compose logs --tail=100 garage-ui
docker compose down
```

`down` preserves data. Adding `--volumes` deletes named volumes; bind-mounted host directories remain on disk.

To change the host port, edit the `ports` mapping. For share links, add a recipient-reachable `S3_PUBLIC_ENDPOINT_URL` under the UI service’s `environment`. See [Deployment](docs/wiki/Deployment.md) for networking, HTTPS, backups, upgrades, Docker without Compose, Linux binaries, and Helm; see [Configuration](docs/wiki/Configuration.md) for the full settings reference.

## What you can do

![Garage UI object browser](docs/images/objects-desktop.png)

<details>
<summary>On a smaller screen</summary>

<img src="docs/images/objects-mobile.png" width="240" alt="Garage UI object browser on a narrow screen">

</details>

- **Files:** browse folders, preview media and text, upload files or folders, download selections as ZIPs, and create expiring share links.
- **Storage:** manage buckets, aliases, quotas, CORS, lifecycle rules, and incomplete uploads.
- **Access:** manage Garage keys and admin tokens; assign UI administrator or viewer roles with separate download and share permissions.
- **Cluster:** inspect health, capacity, nodes, layout, workers, and block errors; run repair tasks.
- **Interface:** light and dark themes, English and Simplified Chinese, desktop and mobile layouts.

Compatibility has been tested with **Garage v2.3.0 and v2.4.1**. Versions older than v2.3.0 cannot perform writes through the UI; newer releases need separate validation. See the [support assessment](docs/garage-support.md) for details.

## Security and persistence

Authentication is always enabled. The first visit creates a UI administrator; the Garage admin token stays on the server. Keep Compose and credential files private and protect remote access with HTTPS.

The UI's `/data` volume contains accounts, sessions, audit records, bucket-key selections, and the session key. Stop the UI before backing up the complete volume. Garage holds the actual objects and cluster state; back up its data and configuration separately. See [backup and restore](docs/wiki/Deployment.md#back-up-and-restore-ui-state).

Release images and binaries are unsigned. Releases include an SBOM and SHA-256 checksums; checksums verify file integrity, not publisher identity.

## Documentation and contributing

- [User guide](docs/wiki/Home.md): first use, configuration, storage, permissions, and cluster operations.
- [Troubleshooting](docs/wiki/Troubleshooting.md): startup errors, login loops, and Garage/S3 connection failures.
- [Contributing](CONTRIBUTING.md): build from source and run focused checks.
- [Support assessment](docs/garage-support.md), [roadmap](docs/roadmap.md), and [verification evidence](docs/README.md#verification-evidence): compatibility, delivery status, and validation limits.
- [Working specifications](docs/README.md): requirements and acceptance targets.

This is a separate project from [Noooste/garage-ui](https://github.com/Noooste/garage-ui).

## License

[MIT](LICENSE).
