# Changelog

Each release section describes its named release or unpublished draft. **Unreleased** lists changes merged to `main` since the latest release; they are not part of any release until a release section names them. Planned work is tracked in the [delivery roadmap](docs/roadmap.md), and verification in [verification evidence](docs/README.md#verification-evidence).

## Unreleased

No unreleased changes.

## 20261023.1

- Update Vite from 6.4.3 to 6.4.5 to fix development-server security advisories, including the `server.fs.deny` query bypass (GHSA-8g5g-mxqh-grgj).
- Update Vitest and its locked internal packages from 4.1.11 to 4.1.12 to fix two mock-server security advisories.

## 20261022.2

- Automatically find an existing, non-expired S3 key with read and write access to each bucket when no project-wide key or explicit bucket selection is configured. No manual selection or new Garage permission grant is required.
- Preserve configured credential precedence, tenant-bound keys, and UI role restrictions. Tenant requests never fall back to another key.
- Update object-access guidance and deployment examples for automatic key discovery.

## 20261022.1

Requires Garage v2.3.0 or later (unchanged).

- Security: update `rustls-webpki` to 0.103.16 and `webpki-root-certs` to 1.0.10 to reject the distrusted emSign certificate paths covered by CVE-2026-111943.
- Copy secrets and complete resource IDs over private-network HTTP, keeping secrets masked after successful copying and providing selection guidance when the browser refuses clipboard access.
- Separate sidebar groups visually and open node maintenance directly. Group maintenance operations by purpose, show their target and consequences, and keep operation feedback attached to the correct node or cluster action.
- Preserve standard storage units, transfer rates, product names, and technical field names in the Simplified Chinese interface.
- Explain missing S3 credentials on object pages and provide direct links to key creation and bucket permissions, with explicit loading, empty, and retry states.
- Permit UI component style attributes, per-page nonced component styles, and two exact Radix viewport styles through CSP while blocking arbitrary inline scripts and other stylesheet elements; preserve the separate sandbox policy on object previews.
- Simplify image-based Docker Compose instructions in both READMEs, document automatic data-directory ownership initialization and shared Garage networking, and make the combined deployment template self-contained.

## 20261020.2

Requires Garage v2.3.0 or later (unchanged).

- Security: validate a session, refresh its activity time, and read its current user within one SQLite transaction. A deleted account's cookie cannot acquire the identity of a new account that reuses its numeric ID.
- Security: clear protected query and mutation caches when signing in, signing out, or losing authentication. Cancel old queries so a delayed response cannot restore the previous account's data.
- Resolve API requests, uploads, object previews, downloads, and audit exports below `BASE_PATH`. Both the prefix root and its trailing-slash form serve the interface, including prefixes such as `/api`.
- Remove partial upload spool files when receiving the body is cancelled or fails. Write spool data asynchronously.
- Read Garage Admin API, fallback detail, and metrics responses incrementally with a 1 MiB limit, including responses without a declared length. Oversized responses fail with `502 garage_unavailable`.
- List incomplete multipart uploads in pages of at most 1,000. The list rejects more than 10,000 entries with `503 multipart_upload_limit` instead of returning a partial result. Abort-all processes each page before requesting the next, keeping memory bounded.

## 20261020.1

Requires Garage v2.3.0 or later (unchanged).

- Creating or rebinding a viewer rejects malformed Garage key-info responses that omit the bucket list or a bucket permissions object, returning `502 garage_unavailable` without changing the account binding. Empty bucket lists and omitted optional write/owner flags remain valid.
- Release asset verification works with both BSD and GNU `find`, including on macOS, while still rejecting unlisted files, hidden entries, and directories.

- CI pins Docker Buildx to v0.38.0 (was v0.25.0) via `docker/setup-buildx-action` in the `build-images` and `release` jobs. v0.25.0 treated `imagetools inspect --format` strings that start with `{{.Manifest` as a plain text dump instead of a Go template (root cause of SAF-49); the template path was fixed from v0.33.0 onward.

**Upgrade note (read before upgrading): this version refuses to start on some existing deployments.** When startup cannot restrict `DATA_DIR` to `0700`, because the directory has the sticky bit, belongs to another user, or the mode change fails, it now stops with `data directory … is writable by other users` (or `by its group`) and `Refusing to start`. Earlier versions logged a warning and started. These deployments are affected:

- `DATA_DIR` is writable by other users and the UI cannot restrict it: always for mode `1777` (sticky, like `/tmp`); for `0777` when the directory belongs to another user, for example a root-owned host path mounted into the container, which runs as UID 65532, or a Kubernetes volume from a local-path or NFS provisioner (see below); and for `0777` owned by the runtime user when changing its mode fails.
- `DATA_DIR` belongs to another user, is writable by its group, and that group is not the process's effective group, for example a Kubernetes volume whose `fsGroup` differs from `runAsGroup`, or a group added with `docker run --group-add`. Supplementary groups do not count as the process's own group.

A directory owned by the runtime user is still changed to `0700` and starts, and so does the image's own `/data` volume. With the Helm chart, the result depends on the StorageClass: the chart's `/data` is a PersistentVolumeClaim from the cluster's default StorageClass, and the pod runs with `runAsUser`, `runAsGroup`, and `fsGroup` all 65532. A volume whose root is not writable by others and to which Kubernetes applies `fsGroup`, as is usual for block storage, ends up in group 65532 and starts. Volumes created with mode `0777` are refused:

- local-path-provisioner, which is the default StorageClass `local-path` in k3s, creates each volume directory as root with mode `0777`. With its default `hostPath` volume type Kubernetes does not apply `fsGroup`, so the pod sees `root:root 0777`. With the `local` type, which the k3s StorageClass selects, `fsGroup` only adds group 65532 and bits, giving `2777`. Both are refused.
- nfs-subdir-external-provisioner creates each directory with mode `0777`, and Kubernetes does not apply `fsGroup` to NFS volumes, so such a volume is refused unless the directory belongs to UID 65532. Other NFS storage may be refused too, depending on the ownership and squashing of the export.

To fix such a volume, give the directory to 65532 on the node or NFS server (`chown 65532:65532 <volume path> && chmod 0700 <volume path>`; local-path volumes are under `/opt/local-path-provisioner`, or `/var/lib/rancher/k3s/storage` in k3s), or run an init container as root that does the same before the UI starts; [Troubleshooting: Kubernetes volumes](https://github.com/xjoker/garage-ui/blob/main/docs/wiki/Troubleshooting.md#kubernetes-volumes) has an example. The chart's defaults are unchanged and it adds no init container. To check before upgrading, run `stat -c 'owner=%u group=%g mode=%a' /path/to/data` and compare the result with the UID and primary GID the UI runs as (65532:65532 in the image and chart). To fix a refused directory, give it to the runtime user (`sudo chown 65532:65532 /path/to/data && sudo chmod 0700 /path/to/data`), or remove group and other write permission (`sudo chmod go-w /path/to/data`). In Kubernetes, set `fsGroup` equal to `runAsGroup`. Do not use a shared directory such as `/tmp`. Details: [Troubleshooting: the data directory is refused at startup](https://github.com/xjoker/garage-ui/blob/main/docs/wiki/Troubleshooting.md#the-data-directory-is-refused-at-startup).

- Behavior change: when startup cannot restrict `DATA_DIR` to `0700`, it checks the directory again and refuses to start if other users, or a group other than the process's effective group, can write to it, because they could replace `garage-ui.sqlite` or `session.key`. The error names the directory. A directory that only the service user and its effective group can write to still starts with a warning, as before; this includes a Kubernetes volume whose group is the pod's `runAsGroup`, as in the Helm chart. Supplementary groups are not checked. The upgrade note above lists the affected deployments and how to fix them.
- Startup no longer changes a shared directory with the sticky bit, such as `/tmp`, or a directory owned by another user; it logs a warning instead, and the rule above applies. On a directory it does change, only the permission bits become `0700`, so a setgid bit is kept.
- An existing `session.key` in `DATA_DIR` with a wider mode, for example one restored from a backup, is changed to `0600` at startup. A file named by `SESSION_KEY_FILE` is not changed.
- The `users`, `user reset-password`, and `user create-admin` commands apply the same data directory rules as startup: they restrict an existing directory to `0700` and stop before opening the database when other accounts can write to it.
- Publishing fails when the `SHA256SUMS` downloaded from the draft differs from the one prepared for the release, always removes its download directory, and reports several drafts with the same tag instead of calling the release published.
- Corrections to the 20261019.1 notes, which stay as published: deleting a user deletes all of its ZIP archive tokens, used or unused, not only unused ones; and the entry about five rate-limit tests describes a test-only change that does not affect the released binaries.
- Publishing no longer stops after pushing `latest` when `latest` already points at the release image. The check that `latest` matches the version tag read each digest through a Buildx output template that Buildx v0.25.0, which the release workflow pinned until SAF-51, answered with its text summary instead, so it never saw a digest and reported both tags as `unavailable` (the 20261019.1 release had to be published by hand). The check now reads the manifest each tag serves and compares their SHA-256 digests. A different digest, or a digest that cannot be read, still stops publication; each failed attempt is logged with its reason, and errors such as timeouts or `503` responses are retried until the time limit.

## 20261019.1

Requires Garage v2.3.0 or later (unchanged). Release checks for this version ran against Garage v2.4.1.

**Upgrade note: every user must sign in again once after upgrading.** The sessions table now stores only the SHA-256 digest of each session cookie. On its first start this version deletes every session written by an earlier version instead of converting it, because a cookie value that was stored in a database file or backup must not stay valid. All open browser sessions, and any script that holds a session cookie, are signed out; accounts, passwords, and roles are unchanged. The startup log records the deletion as `signed out sessions stored before session digests` with the number of sessions removed.

- Behavior change: a viewer can no longer be bound to a Garage key that can write. `POST /api/users` and `PATCH /api/users/{id}` read the key's bucket permissions from Garage and refuse a key that holds `write` or `owner` on any bucket with `403 key_exceeds_role`. A viewer that is already bound to such a key stays bound but is read-only: uploads, deletes, and incomplete-upload aborts return `403 viewer_read_only` before any S3 request, and reads keep working. Rebind those accounts to a read-only key, or make them administrators, if they need to change objects.
- Behavior change: a recursive object delete treats `key` as a folder and appends `/` when it is missing. `key=logs-2024&recursive=true` now deletes only objects under `logs-2024/`; it no longer deletes `logs-2024.txt` or other keys that merely start with `logs-2024`. A request whose folder has no objects, including such a legacy prefix request, returns `200` with `{"deleted":0}` and no error, so scripts that relied on the old prefix match must pass the folder they mean or delete those keys individually.
- Add `PORT` (default `3909`): the TCP port the UI listens on, 1–65535. An invalid value stops startup with an error naming `PORT`; `--health` probes the same port. Both Compose files pass `PORT` to the container and publish it on the same loopback port (`127.0.0.1:${PORT:-3909}`), and the Helm chart adds a `port` value (default `3909`) for the container, Service, and `PORT`. Existing deployments that do not set `PORT` keep listening on 3909.
- Behavior change: startup sets `DATA_DIR` to mode `0700`, and the process creates files with a `0077` umask, so files it creates there, such as a generated `session.key`, are `0600` (the SQLite database files are also kept at `0600`; see the entry on `garage-ui.sqlite` below). Other local users can no longer list or read the data directory; backup or monitoring jobs that read it as another user need to run as the service user. A directory the process cannot change, such as a mount point owned by another user, is left as it is with a warning.
- `SHA256SUMS` lists the environment template as `default.env.example`, the name of the release asset, instead of `.env.example`. `sha256sum --check --ignore-missing SHA256SUMS` now verifies a downloaded `default.env.example` instead of silently skipping it. Publishing now downloads the uploaded draft assets into an empty directory and fails unless they are exactly the files named in `SHA256SUMS` and `sha256sum --check --strict` passes; the release workflow repeats the check on the published release (`scripts/verify-release-assets.sh`).
- A failed upload shows why in its row of the upload panel, for example the localized `viewer_read_only` or `key_exceeds_role` message, instead of only **Failed**.
- Add `LOGIN_IP_LIMIT_PER_MINUTE` (default `30`): total login attempts per minute from one client address, across all usernames. Every login request spends this budget first, including requests whose username no account can hold. The client address follows `TRUSTED_PROXY`; IPv6 addresses in one /64 network share one budget. Exceeding the limit answers `429` with `Retry-After`.
- Key the setup limiter by the same client-address key as the login address budget, so IPv6 addresses in one /64 network share one setup budget (an IPv4-mapped IPv6 address counts as its IPv4 address). The setup limit itself is unchanged: a fixed 10 attempts per minute per address, not configurable and not affected by `LOGIN_IP_LIMIT_PER_MINUTE`; exceeding it answers `429`.
- Condense unauthenticated audit noise that a client can cause without naming a possible account. Only these rows are condensed: `401` login rows for an illegal username, `429` rows from the login address, per-username login, or setup limiters (including `429`s for a well-formed username), and `409 already_initialized` rows for setup requests after the first administrator exists. Up to 10 such rows per client address per minute, shared across login and setup, are written individually; further rows are counted and written about once a minute as one `audit_suppressed` summary per address and path. Failed logins for well-formed usernames, successful logins, and authenticated actions are always written in full.
- Behavior change: the UI's own `/metrics` endpoint accepts only the metrics token (`Authorization: Bearer <GARAGE_METRICS_TOKEN>`) or a session whose role may view metrics, the same rule as `/api/metrics`. Administrators and viewers keep access; a viewer bound to an access key (a tenant) now receives `403 forbidden` instead of the metrics text. Unauthenticated requests still receive `401`, and scrapers that use the metrics token are unaffected.
- Behavior change: the sessions table stores only the SHA-256 digest of each session cookie. The first start after upgrading deletes the sessions written by earlier versions, so every signed-in user must log in again once. Sessions past their absolute or idle expiry, or issued under a previous session key, are now deleted at startup and every five minutes instead of only when they are presented.
- Behavior change: `garage-ui.sqlite` and its `-wal`, `-shm`, and `-journal` sidecars are created with mode `0600`, and existing files are changed to `0600` at startup (a file that cannot be changed is logged as a warning). Backup or monitoring jobs that read these files as a different user need to run as the service user or read a copy.
- Release builds no longer contain the test-only `GET /api/objects/preview`, `GET /api/objects/download`, and `POST /api/objects` routes. They served only in-memory test fixtures or answered `501`; object access uses the `/api/buckets/{id}/...` routes, so the UI is unaffected. These paths now behave like any other unknown path.
- The Users page asks for confirmation before deleting an account; **Cancel** or Escape keeps it.
- Audit the `403 viewer_read_only` refusal of incomplete-upload aborts (`DELETE /api/buckets/{id}/multipart`) as a `denied` row, like the refused uploads and deletes, so every refused object change by a bound viewer is audited.
- Deleting a user also deletes its sessions and unused ZIP archive tokens in the same transaction, instead of relying only on SQLite `ON DELETE CASCADE`. User ids can be reused, so a session left behind (for example with foreign keys off) could otherwise sign in as the next account created with that id. A deleted user's cookies stop working immediately.
- Release publication waits up to 300 seconds, instead of 60, for GHCR to resolve the `latest` image tag to the verified release image, and bounds each registry query to 10 seconds.
- Add a Simplified Chinese README (`README.zh-CN.md`). The README quick start downloads the environment template as `default.env.example`, its release asset name, and saves it as `.env`.
- Run five rate-limit tests on a frozen limiter clock, so they no longer fail on a slow or heavily loaded machine.

## 20261016.3

First public release. Requires Garage v2.3.0 or later; browser integration is verified against Garage v2.3.0 and v2.4.1.

- Manage buckets, keys, grants, aliases, quotas, CORS, lifecycle rules, and scoped administrator tokens.
- Browse, preview, upload, share, and download objects, including multipart transfers and ZIP archives; cancel transfers and delete individual objects, selections, or prefixes.
- Configure the first administrator and manage users, roles, sessions, and audit exports. Validate bucket credentials and update their mapping and audit record atomically; prevent invalidated credentials from being restored by concurrent cache loads.
- Inspect cluster health, metrics, layout history, workers, and block errors, and run guarded maintenance operations.
- Provide responsive English and Simplified Chinese interfaces, light and dark themes, keyboard navigation, and accessible controls. Preserve concurrent upload completion states and readable light-theme accent text.
- Verify browser workflows in Chromium, Firefox, and WebKit, including upload cancellation, ZIP downloads, video seeking, setup, and TLS warnings.
- Provide a Docker-first quick start with standalone Compose and configuration downloads; build each architecture on a native runner only from version tags that match the repository manifests, and wait for the published latest image tag to match the verified release image. Tests run locally before tagging.
- Publish static Linux amd64 and arm64 binaries, multi-architecture container images, an SPDX SBOM, and SHA-256 checksums. Images and binaries are unsigned; signing and build-provenance attestations are deferred. Release-image health reports identify the exact source commit and version.

## 20261015.1 (DRAFT)

Unpublished release draft following 20261014.1. This update expands storage, access management, cluster administration, and observability, and migrates the embedded React 18 interface to Radix Themes. All pages use compact desktop and mobile layouts with updated Garage UI branding. The minimum supported Garage version remains v2.3.0, with compatibility verified on Garage v2.3.0 and v2.4.1.

- Apply a previewed layout at the next version, revert staged changes, and pass the current layout version when skipping dead nodes, with additional confirmation before allowing missing data. Expose layout history, metrics, worker and block diagnostics, and repair controls.
- Expand key grants, bucket CORS and lifecycle editing, object inspection, incomplete-upload cleanup counts, audit export, and admin-token warnings.
- Show bitmap thumbnails only when download is permitted, navigate adjacent objects with Left and Right, and collapse the upload panel to keep object rows visible. Preserve bucket refreshes during the initial list load and keep object keys ending in `/thumbnail` accessible.
- Provide English and Simplified Chinese UI catalogs, including navigation, forms, dialogs, timestamps, and capacity units. Use accessible Radix controls, menus, tabs, and dialogs throughout navigation, account management, storage, access, and cluster pages.
- Preserve authorization revocations during password changes and resets, and rotate password-change sessions atomically. Update frontend dependencies and rustls to resolve security advisories. Add regression checks for HTTP transport timeouts, graceful shutdown, viewer permissions, revoked ZIP tokens, browser preflight, and video seeking.
- Retry failed S3 endpoint discovery in the same process after the Admin API recovers, retaining configured overrides. Resolve credentials without holding a lock during network requests, and prevent stale retries from restoring invalidated credentials.
- Report fatal ZIP download failures as failed transfers and cancel pending archive work when a download is abandoned; recoverable failures opening selected objects remain listed in `_errors.txt`.
- Preserve successful and failed node outcomes for maintenance operations. Show partial and complete failures accurately, and record rejected upstream mutations as audit errors.
- Invalidate bucket and key pagination cursors after catalog mutations, and prevent older responses from restoring stale page boundaries.
- Read audit pages directly with bounded SQL queries and stream CSV exports in bounded batches. Neutralize formula-leading CSV text without changing stored records or JSON results.
- Embed the source commit in official builds and require release-image health checks to match the exact version and commit. Share the image-build interface with unsigned release preparation, and support local Linux amd64 and arm64 builds through `scripts/build.sh --images`.
- Add repeatable list-performance, ZIP-memory, and large-file thumbnail checks, and redact session cookies from browser-check failure logs.
- Expand the operator Wiki and AI contribution guide with current interface screenshots, task-oriented workflows, configuration, and verification guidance.

## 20261014.1 (DRAFT)

Unpublished initial release draft. The minimum Garage version is v2.3.0. This version was verified on v2.3.0 and v2.4.1.

- Set up the first administrator, log in, log out, and change a password. Roles are admin and viewer, with separate download and share permission bits.
- List, create, and delete buckets. Set aliases and quotas. Grant and revoke bucket permissions.
- List, create, delete, and rename keys. Allow a key to create buckets. A key secret is shown once, after a second confirmation.
- Browse objects by prefix. Preview allowlisted types inline. Download and upload objects, including multipart upload. Use the upload panel. Delete one object, delete a selection after typing the count, and delete a prefix recursively. Filter, sort, and select objects.
- Choose a light theme, a dark theme, or the system theme. Loading, empty, error, and retry states are part of those flows.
- Read cluster health, status, and statistics. Open the About page.
