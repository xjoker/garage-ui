# Changelog

Each release section describes its named release or unpublished draft. **Unreleased** lists changes merged to `main` since the latest release; they are not part of any release until a release section names them. Planned work is tracked in the [delivery roadmap](docs/roadmap.md), and verification in [verification evidence](docs/README.md#verification-evidence).

## Unreleased

Changes on `main` after 20261016.3 that are not yet part of a release.

- Behavior change: a viewer can no longer be bound to a Garage key that can write. `POST /api/users` and `PATCH /api/users/{id}` read the key's bucket permissions from Garage and refuse a key that holds `write` or `owner` on any bucket with `403 key_exceeds_role`. A viewer that is already bound to such a key stays bound but is read-only: uploads, deletes, and incomplete-upload aborts return `403 viewer_read_only` before any S3 request, and reads keep working. Rebind those accounts to a read-only key, or make them administrators, if they need to change objects.
- Behavior change: a recursive object delete treats `key` as a folder and appends `/` when it is missing. `key=logs-2024&recursive=true` now deletes only objects under `logs-2024/`; it no longer deletes `logs-2024.txt` or other keys that merely start with `logs-2024`. A request whose folder has no objects, including such a legacy prefix request, returns `200` with `{"deleted":0}` and no error, so scripts that relied on the old prefix match must pass the folder they mean or delete those keys individually.
- Add `LOGIN_IP_LIMIT_PER_MINUTE` (default `30`): total login attempts per minute from one client address, across all usernames. Every login request spends this budget first, including requests whose username no account can hold. The client address follows `TRUSTED_PROXY`; IPv6 addresses in one /64 network share one budget. Exceeding the limit answers `429` with `Retry-After`.
- Key the setup limiter by the same client-address key as the login address budget, so IPv6 addresses in one /64 network share one setup budget (an IPv4-mapped IPv6 address counts as its IPv4 address). The setup limit itself is unchanged: a fixed 10 attempts per minute per address, not configurable and not affected by `LOGIN_IP_LIMIT_PER_MINUTE`; exceeding it answers `429`.
- Condense unauthenticated audit noise that a client can cause without naming a possible account. Only these rows are condensed: `401` login rows for an illegal username, `429` rows from the login address, per-username login, or setup limiters (including `429`s for a well-formed username), and `409 already_initialized` rows for setup requests after the first administrator exists. Up to 10 such rows per client address per minute, shared across login and setup, are written individually; further rows are counted and written about once a minute as one `audit_suppressed` summary per address and path. Failed logins for well-formed usernames, successful logins, and authenticated actions are always written in full.
- Behavior change: the UI's own `/metrics` endpoint accepts only the metrics token (`Authorization: Bearer <GARAGE_METRICS_TOKEN>`) or a session whose role may view metrics, the same rule as `/api/metrics`. Administrators and viewers keep access; a viewer bound to an access key (a tenant) now receives `403 forbidden` instead of the metrics text. Unauthenticated requests still receive `401`, and scrapers that use the metrics token are unaffected.
- Behavior change: the sessions table stores only the SHA-256 digest of each session cookie. The first start after upgrading deletes the sessions written by earlier versions, so every signed-in user must log in again once. Sessions past their absolute or idle expiry, or issued under a previous session key, are now deleted at startup and every five minutes instead of only when they are presented.
- Behavior change: `garage-ui.sqlite` and its `-wal`, `-shm`, and `-journal` sidecars are created with mode `0600`, and existing files are changed to `0600` at startup (a file that cannot be changed is logged as a warning). Backup or monitoring jobs that read these files as a different user need to run as the service user or read a copy.
- Release builds no longer contain the test-only `GET /api/objects/preview`, `GET /api/objects/download`, and `POST /api/objects` routes. They served only in-memory test fixtures or answered `501`; object access uses the `/api/buckets/{id}/...` routes, so the UI is unaffected. These paths now behave like any other unknown path.
- The Users page asks for confirmation before deleting an account; **Cancel** or Escape keeps it.
- Audit the `403 viewer_read_only` refusal of incomplete-upload aborts (`DELETE /api/buckets/{id}/multipart`) as a `denied` row, like the refused uploads and deletes, so every refused object change by a bound viewer is audited.

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
