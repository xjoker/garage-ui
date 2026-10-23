<p align="center">
  <img src="web/public/brand/garage-ui.svg" width="80" alt="Garage UI logo">
</p>

# Garage UI

[![Releases](https://img.shields.io/badge/Releases-GitHub-blue)](https://github.com/xjoker/garage-ui/releases)

**Manage your Garage cluster, buckets, and files in one browser.**

English | [简体中文](README.zh-CN.md)

An unofficial web interface for [Garage](https://garagehq.deuxfleurs.fr/), the S3-compatible object store. Browse and upload files, manage access, and check cluster health. One container includes the server and web interface; no separate database is required.

## Docker Compose

Save this as `compose.yaml`, replace the four Garage values, and run `docker compose up -d`. It uses the published image directly; no repository checkout, build, or `.env` file is required.

```yaml
services:
  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261023.3
    restart: unless-stopped
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://garage:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://garage:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
    volumes:
      - garage-ui-data:/data
volumes:
  garage-ui-data:
```

`garage` is the service name in the example. For Garage outside this Compose project, use an address that the UI container can reach. The Admin token comes from Garage's configuration and is not an S3 access key. The named volume is initialized for the non-root UI automatically. If you use a host directory instead, give it to UID/GID `65532:65532` and keep it dedicated to the UI.

### Run Garage and the UI together

For a new single-node Garage, place your `garage.toml` beside `compose.yaml` and use:

```yaml
services:
  garage:
    image: dxflrs/garage:v2.4.1
    restart: unless-stopped
    command: ["/garage", "server", "--single-node"]
    volumes:
      - ./garage.toml:/etc/garage.toml:ro
      - garage-data:/var/lib/garage

  garage-ui:
    image: ghcr.io/xjoker/garage-ui:20261023.3
    restart: unless-stopped
    depends_on:
      - garage
    ports:
      - "127.0.0.1:3909:3909"
    environment:
      GARAGE_ADMIN_URL: http://garage:3903
      GARAGE_ADMIN_TOKEN: replace-with-your-garage-admin-token
      S3_ENDPOINT_URL: http://garage:3900
      S3_REGION: garage
      COOKIE_SECURE: "false"
    volumes:
      - garage-ui-data:/data

volumes:
  garage-data:
  garage-ui-data:
```

Set the UI token and region to match `garage.toml`. Garage must listen on `0.0.0.0:3903` for its Admin API and `0.0.0.0:3900` for S3; keep its data and metadata paths under `/var/lib/garage` or adjust the mounts. See Garage's [configuration quick start](https://garagehq.deuxfleurs.fr/documentation/quick-start/). This example has one node and no data redundancy. Garage APIs stay inside the Compose network; publish S3 separately if other clients need access.

### Start and open the UI

```sh
docker compose up -d
```

Open **[http://localhost:3909](http://localhost:3909)** and create the first UI administrator. The UI has its own login: choose the username and password on this screen. Garage admin tokens and S3 keys are not UI login credentials.

Existing buckets automatically use a compatible read-write S3 key. For a new bucket, create or select a key and grant it access. Viewer accounts remain read-only; accounts bound to an S3 key keep that key’s grants. Download and share permissions remain separate.

The examples use local HTTP. For remote access, configure [HTTPS](docs/wiki/Deployment.md#serve-the-ui-over-https) and change `COOKIE_SECURE` to `"true"`. For a remote Docker host, use `ssh -L 3909:127.0.0.1:3909 your-server` to access the local port.

## Manage the deployment

Run these commands in the directory containing `compose.yaml`:

```sh
docker compose ps
docker compose logs --tail=100 garage-ui
docker compose down
```

`down` preserves data. Adding `--volumes` deletes named volumes; bind-mounted host directories remain on disk.

For HTTPS, set `COOKIE_SECURE: "true"` and place a reverse proxy in front of the loopback port. For share links, add a recipient-reachable `S3_PUBLIC_ENDPOINT_URL`. See [Deployment](docs/wiki/Deployment.md) for networking, backups, upgrades, binaries, and Helm; see [Configuration](docs/wiki/Configuration.md) for every setting.

## What you can do

- **Files:** browse folders, preview media and text, upload files or folders, download selections as ZIPs, and create expiring share links.
- **Storage:** manage buckets, aliases, quotas, CORS, lifecycle rules, and incomplete uploads.
- **Access:** manage Garage keys and admin tokens; assign UI administrator or viewer roles with separate download and share permissions.
- **Cluster:** inspect health, capacity, nodes, layout, workers, and block errors; run repair tasks.
- **Interface:** light and dark themes, English and Simplified Chinese, desktop and mobile layouts.

Compatibility has been tested with **Garage v2.3.0 and v2.4.1**. Versions older than v2.3.0 cannot perform writes through the UI; newer releases need separate validation. See the [support assessment](docs/garage-support.md) for details.

## Security and persistence

Authentication is always enabled. The first visit creates a UI administrator; the Garage admin token stays on the server. Keep Compose files containing tokens private and protect remote access with HTTPS.

The UI's `/data` volume contains accounts, sessions, audit records, bucket-key selections, and the session key. Stop the UI before backing up the complete volume. Garage holds the actual objects and cluster state; back up its data and configuration separately. See [backup and restore](docs/wiki/Deployment.md#back-up-and-restore-ui-state).

Release images and binaries are unsigned. Releases include an SBOM and SHA-256 checksums; checksums verify file integrity, not publisher identity.

## Documentation and contributing

- [User guide](docs/wiki/Home.md): first use, configuration, storage, permissions, and cluster operations.
- [Troubleshooting](docs/wiki/Troubleshooting.md): startup errors, login loops, and Garage/S3 connection failures.
- [Contributing](CONTRIBUTING.md): build from source and run focused checks.
- [Support assessment](docs/garage-support.md), [roadmap](docs/roadmap.md), and [verification evidence](docs/README.md#verification-evidence): compatibility, delivery status, and validation limits.

This is a separate project from [Noooste/garage-ui](https://github.com/Noooste/garage-ui).

## License

[MIT](LICENSE).
