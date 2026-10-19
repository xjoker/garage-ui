# Troubleshooting

Capture the visible error message/code, the affected task, and the UI/Garage versions before retrying. Keep tokens, passwords, and object content out of screenshots and issue reports.

## The UI does not start

| Symptom | Check |
|---|---|
| Secret-file error | The file exists and is readable by the runtime user, including UID 65532 in the container |
| Secret rejected | Supply the real token/key; documented placeholder values, all-zero values, and short keys are rejected |
| Bootstrap import error | Set username and bcrypt hash together; use a valid username and hash |
| Object-key configuration error | Set both S3 access key ID and secret, or neither |
| Authentication-disabled setting | Remove `AUTH_DISABLED`, `DISABLE_AUTH`, and `GARAGE_UI_NO_AUTH` entirely |
| Data directory not writable | Make `DATA_DIR` or its mounted volume writable by the runtime user |
| `data directory … is writable by other users` (or `by its group`) at startup | The UI could not restrict `DATA_DIR` to `0700`, and other accounts could replace its database or session key. Give the directory to the runtime user (UID 65532 in the image), or remove group and other write permission (`chmod go-w`). A group-writable directory is accepted only when its group is the runtime user's effective (primary) group, as with a Kubernetes `fsGroup` equal to `runAsGroup`; a supplementary group does not count. A Kubernetes volume created with mode `0777`, for example by local-path or NFS provisioners, is refused even with the Helm chart. See [The data directory is refused at startup](#the-data-directory-is-refused-at-startup) |
| Listener error | `HOST` must be an IP address, `PORT` must be a TCP port from 1 to 65535 (default 3909), and that port must be available |

The local health probe is:

```sh
./target/release/garage-ui --health
```

It checks `/healthz` on 127.0.0.1 at `PORT` (default 3909) and exits successfully when the probe succeeds. A successful UI liveness check does not prove that Garage is reachable or all S3 operations work.

## The data directory is refused at startup

Startup sets `DATA_DIR` to mode `0700`. When it cannot (the directory has the sticky bit, is owned by another user, or the mode change fails), it checks who else can write to the directory and stops with `data directory … is writable by other users` or `… is writable by its group`, followed by `Refusing to start`. Earlier versions only logged a warning, so a deployment that started before can stop after an upgrade.

The directory is refused when the UI cannot restrict it and either:

- other users can write to it (`o+w`), for example mode `0777` or `1777`, which includes the sticky bit and directories like `/tmp`; or
- its group can write to it (`g+w`) and its group is not the process's effective group (egid).

Only the effective group is compared. Supplementary groups are not checked. A group added with `docker run --group-add`, or a Kubernetes `fsGroup` that differs from `runAsGroup`, does not count as the service's own group even though the process can write through it, because every other member of that group could write too.

Check the directory and the identity the UI runs as:

```sh
stat -c 'owner=%u group=%g mode=%a' /path/to/data
```

The image and the Helm chart run as UID 65532 with primary group 65532 (`runAsUser`, `runAsGroup`, and `fsGroup` are all 65532 in the chart). With Docker, the identity is the image user or `--user UID:GID`. In Kubernetes, it is the pod's `runAsUser` and `runAsGroup`. The directory is refused if it is not owned by that UID, or has the sticky bit (an odd first digit in a four-digit mode, such as `1777`), or changing its mode fails, and it is writable by others or by a group other than that GID.

### Kubernetes volumes

With the Helm chart, `/data` is a PersistentVolumeClaim from the cluster's default StorageClass, so whether the chart's defaults start depends on how that StorageClass creates volumes. Kubernetes applies `fsGroup` only to some volume types, and then only adds group 65532, group read and write, and the setgid bit; it never removes write permission for others.

- Block storage whose file system root is root-owned mode `0755`, with `fsGroup` applied, becomes `root:65532 2775` and starts (with a warning that the directory is left unchanged).
- local-path-provisioner, the default StorageClass `local-path` in k3s, creates each volume as root with mode `0777`. Its default `hostPath` volume type gets no `fsGroup`, so the pod sees `root:root 0777`; the `local` type, which k3s selects, becomes `root:65532 2777`. Both are refused.
- nfs-subdir-external-provisioner creates directories with mode `0777`, and NFS volumes get no `fsGroup`, so they are refused unless the directory belongs to UID 65532. Other NFS storage may be refused too, depending on the export's ownership and squashing.

Check a running or failed pod's volume on the node with `stat` on the volume path: local-path volumes are under `/opt/local-path-provisioner`, or `/var/lib/rancher/k3s/storage` in k3s. Fix it in one of two ways:

- On the node or NFS server, give the directory to the runtime user: `sudo chown 65532:65532 <volume path> && sudo chmod 0700 <volume path>`. This lasts as long as the volume.
- Add an init container that runs as root and does the same before the UI starts. The chart has no setting for init containers, so add it with a Helm post-renderer or a kustomize patch on the Deployment; a `kubectl patch` is undone by the next `helm upgrade`. Pod Security Admission at the `restricted` level rejects a container that runs as root, and with NFS `root_squash` the `chown` fails, so fix the directory on the server instead in those cases.

```yaml
# Add under spec.template.spec of the Deployment named after the Helm release.
initContainers:
  - name: data-permissions
    image: busybox:1.37
    command: ["sh", "-c", "chown 65532:65532 /data && chmod 0700 /data"]
    securityContext:
      runAsUser: 0
      runAsNonRoot: false   # the pod sets runAsNonRoot: true
    volumeMounts:
      - name: data
        mountPath: /data
```

On a volume that gets `fsGroup`, Kubernetes adds group bits again when the pod next starts; the directory then still belongs to 65532, so the UI changes it back to `0700` itself.

Fix it in one of these ways:

```sh
# Give the directory to the runtime user; startup then restricts it to 0700 itself.
sudo chown 65532:65532 /path/to/data
sudo chmod 0700 /path/to/data

# Or keep the owner and remove group and other write permission.
sudo chmod go-w /path/to/data
```

- Kubernetes: set `fsGroup` to the same value as `runAsGroup`, as the Helm chart does, or remove group write permission from the volume. A volume that others can write to, such as a local-path or NFS volume with mode `0777`, needs one of the fixes in [Kubernetes volumes](#kubernetes-volumes).
- Docker with a shared group: run with `--user UID:GID`, using the directory's group as the primary group, instead of adding it with `--group-add`.
- Do not use a shared directory such as `/tmp` as `DATA_DIR`. Use a directory dedicated to the UI instead of changing the mode of a shared one.

## Login loops or setup reappears

For intentional local HTTP, set `COOKIE_SECURE=false`. For HTTPS keep it true, and make sure the proxy serves the configured `BASE_PATH` consistently. Secure cookies should not be weakened to fix a production proxy error.

A lost data volume can make setup reappear because the UI has no users. Stop and restore the intended state before creating a new account if that is unexpected. Do not delete the database to recover a password; use [the recovery commands](Users-and-authentication.md#recover-access-from-the-command-line).

Session expiry, a password reset, account disablement, or a permission change can invalidate a login. Sign in again once the account and permissions are correct.

## Garage is unavailable or operations are disabled

Check the admin URL from the UI server's network, the configured token/scopes, and Garage's TLS certificate. Use `GARAGE_TLS_CA_FILE` for a private CA. The UI clearly warns when `GARAGE_TLS_INSECURE=1`; this setting disables certificate verification.

Garage older than v2.3.0 is blocked from writes. A scoped Garage admin token may permit overview reads but deny layout, key, or token operations. A UI administrator still needs the server's Garage token to permit the requested endpoint.

Metrics have their own token and unavailable notice. Repair the metrics configuration separately if cluster health is available but metrics are not.

## Buckets load but objects fail

Verify `S3_ENDPOINT_URL` and `S3_REGION` against Garage's S3 configuration. Explicit settings are necessary for v2.3.0 and v2.4.1. The S3 endpoint must be reachable by the UI process, not merely by your browser.

Choose a **Bucket key** when prompted, or check the configured project-wide S3 key. Verify Read/Write grants for that bucket. A project-wide key overrides a per-bucket choice. An admin token is not an S3 credential. After restoring an unavailable Garage Admin API, retry the object operation in the same UI process: failed S3 discovery probes are not retained until restart. Explicit endpoint/region settings still take precedence.

A pending credential lookup, upload, or read releases the shared credential-cache lock before network I/O, allowing other buckets and permission changes to proceed. When a key changes during credential loading, object operations and share-link signing reload their credentials before filling the cache. A revoked or replaced credential cannot be restored by an in-flight forbidden retry. Check current grants and retry with the intended key. This does not cancel an operation Garage has already accepted.

## Preview, download, share, or upload fails

| Task | Check |
|---|---|
| Preview/download missing for a viewer | The viewer needs Download permission; unsupported content types are not inline previews |
| Share unavailable | Viewer needs both Download and Share; server needs a public S3 endpoint |
| Share link cannot open | Use a recipient-reachable public S3 hostname, the correct region, and an unexpired link |
| ZIP link fails or its stream is interrupted | Check Garage/S3 connectivity and prepare a new ZIP link; discard an incomplete archive. Tokens are short-lived, single-use, and session-user-bound |
| Upload fails | Check file limit, bucket quota, Write grants, connectivity, and queue destination; use Retry after fixing the cause |
| Upload appears in another location | Individual-file keys use filenames; folder uploads use relative folder paths rather than the currently browsed prefix |
| Deletion partially fails | Read remaining per-key errors and the refreshed list before retrying |

Bucket/key catalog mutations invalidate cached pagination boundaries. Refreshing a boundary also discards later offsets; an older in-flight response cannot restore an invalidated cursor. Refresh the list after a change before relying on a later page.

Size/time sorting and many list filters apply to loaded rows or the current page. Use pagination or load more; narrow an object search if it reports truncation.

For a cluster-wide partial failure, follow [Maintenance](Maintenance.md#handle-an-all-node-failure) before retrying successful nodes.

## Report a reproducible problem

Include what you tried, expected and actual results, exact non-secret error text, UI version/source commit, Garage version, browser, and whether deployment is a binary, Compose, or Helm. Add only the relevant redacted settings. Use the [repository issue tracker](https://github.com/xjoker/garage-ui/issues).
