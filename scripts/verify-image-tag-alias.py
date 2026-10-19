#!/usr/bin/env python3
"""Wait for a published tag alias to resolve to the verified version image.

Both tags are read with `docker buildx imagetools inspect --raw`, which prints the
manifest (or index) exactly as the registry serves it, and the digest is the SHA-256 of
those bytes. This does not depend on `--format`: Buildx before v0.33.0 (the release
workflow pinned v0.25.0 until SAF-51) ignored any template that starts with `{{.Manifest`
and printed the human-readable summary instead, so `{{.Manifest.Digest}}` never produced
a digest there.

The gate is unchanged: the alias passes only when both tags were read in the same attempt
and resolve to the same well-formed digest. A mismatch, or a tag whose digest cannot be
read, fails once the timeout expires. Every failed attempt is logged with its reason.
"""

import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import time

# One registry read; a slow read is reported and retried instead of shortening later ones.
ATTEMPT_TIMEOUT = 20
# Longest wait between attempts after consecutive transient errors.
MAX_BACKOFF = 15
MANIFEST_MEDIA_TYPES = {
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
    "application/vnd.oci.image.manifest.v1+json",
    "application/vnd.docker.distribution.manifest.v2+json",
}
# Errors that waiting cannot fix. Anything else, including errors not listed in either
# group, is retried until the timeout, and still fails if it never clears.
# Registry 401/403 answers carry the words below ("401 Unauthorized", "403 Forbidden", "denied");
# a bare 401 or 403 is not matched, so text such as "read 401 bytes" is not taken for one.
PERMANENT_ERRORS = re.compile(
    r"unauthorized|authentication required|denied|forbidden|invalid reference format"
    r"|unknown flag|unknown command|is not a docker command|unknown shorthand flag",
    re.IGNORECASE,
)
# Credentials that could appear in docker output, each replaced before anything is logged.
# Every rule has its own test case in scripts/check-release-version.sh.
SECRET_KEY = r"[\w.-]*(?:token|secret|password|passwd|api[_-]?key)"
SECRETS = [
    ("bearer-basic", re.compile(r"(?i)\b(bearer|basic)\s+[^\s\"']+"), r"\1 [redacted]"),
    ("key-value", re.compile(rf"(?i)\b({SECRET_KEY})(=|%3D)[^\s&\"',;]+"), r"\1\2[redacted]"),
    ("json-field", re.compile(rf"(?i)(\"{SECRET_KEY}\"\s*:\s*\")[^\"]*\""), r'\1[redacted]"'),
    ("password-colon", re.compile(r"(?i)\b(passw(?:or)?d)(\s*:\s*)[^\s,;\"']+"), r"\1\2[redacted]"),
    ("url-userinfo", re.compile(r"(?i)\b([a-z][a-z0-9+.-]*://)[^/\s:@]+:[^/\s@]+@"), r"\1[redacted]@"),
    ("jwt", re.compile(r"\beyJ[\w-]+\.[\w-]+\.[\w-]*"), "[redacted]"),
    ("github-token", re.compile(r"\b(gh[pousr]_[A-Za-z0-9]{20,}|github_pat_\w{20,})"), "[redacted]"),
]


class Reading:
    """The outcome of reading one tag: a digest, or why there is none."""

    def __init__(self, digest="", error="", transient=True):
        self.digest = digest
        self.error = error
        self.transient = transient

    def describe(self):
        if self.digest:
            return self.digest
        kind = "transient" if self.transient else "permanent"
        return f"unavailable ({kind}: {self.error})"


def summarize(text, limit=300):
    """One redacted line from command output, short enough for a CI log."""
    for _name, pattern, replacement in SECRETS:
        text = pattern.sub(replacement, text)
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 3] + "..."


def digest_of(raw):
    """The digest of a manifest exactly as served, or an error naming what was printed.

    Buildx exited successfully, so output that is not a manifest will not change on a retry
    (for example a Buildx version that prints something else); it fails at once.
    """
    try:
        manifest = json.loads(raw)
    except ValueError:
        detail = summarize(raw.decode("utf-8", "replace"), 120)
        return Reading(error=f"output is not a manifest: {detail!r}", transient=False)
    if not isinstance(manifest, dict):
        return Reading(error="output is not a manifest object", transient=False)
    media_type = manifest.get("mediaType")
    if media_type not in MANIFEST_MEDIA_TYPES and not (
        media_type is None and manifest.get("schemaVersion") == 2 and ("manifests" in manifest or "config" in manifest)
    ):
        return Reading(error=f"unsupported manifest media type {media_type!r}", transient=False)
    return Reading(digest="sha256:" + hashlib.sha256(raw).hexdigest())


def inspect_digest(reference, timeout=None):
    """Read the digest that a registry currently serves for a reference."""
    timeout = ATTEMPT_TIMEOUT if timeout is None else timeout
    command = ["docker", "buildx", "imagetools", "inspect", "--raw", reference]
    try:
        # A session of its own lets a timeout stop the Buildx plugin process too, not only docker.
        process = subprocess.Popen(
            command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True
        )
    except FileNotFoundError:
        return Reading(error="docker is not installed or not on PATH", transient=False)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate()
        return Reading(error=f"no response within {timeout:g} seconds")
    stderr = summarize(stderr.decode("utf-8", "replace"))
    if process.returncode != 0:
        detail = f"exit status {process.returncode}: {stderr or 'no error output'}"
        return Reading(error=detail, transient=not PERMANENT_ERRORS.search(stderr))
    return digest_of(stdout)


def log(message):
    print(message, file=sys.stderr, flush=True)


def verify_alias(registry, version_tag, alias_tag, timeout=300, poll_interval=3):
    version = f"{registry}:{version_tag}"
    alias = f"{registry}:{alias_tag}"
    deadline = time.monotonic() + timeout
    attempt = 0
    errors_in_a_row = 0
    last_version = last_alias = Reading(error="not read")
    while True:
        attempt += 1
        last_version = inspect_digest(version)
        last_alias = inspect_digest(alias)
        if last_version.digest and last_version.digest == last_alias.digest:
            print(f"Verified {alias} matches {version} ({last_version.digest})")
            return True

        failed = [reading for reading in (last_version, last_alias) if not reading.digest]
        if failed:
            errors_in_a_row += 1
            delay = min(poll_interval * 2 ** (errors_in_a_row - 1), MAX_BACKOFF)
        else:
            errors_in_a_row = 0
            delay = poll_interval
        log(
            f"Attempt {attempt}: {alias} not verified "
            f"(version={last_version.describe()}, alias={last_alias.describe()})"
        )
        permanent = [reading for reading in failed if not reading.transient]
        if permanent:
            log(f"Not retrying: {permanent[0].error}")
            break
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        time.sleep(min(delay, remaining))
        if time.monotonic() >= deadline:
            break
    log(
        f"{alias} did not converge to {version} within {timeout} seconds after {attempt} attempts "
        f"(version={last_version.describe()}, alias={last_alias.describe()})"
    )
    return False


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(f"Usage: {sys.argv[0]} <registry> <version-tag> <alias-tag>")
    raise SystemExit(0 if verify_alias(*sys.argv[1:]) else 1)
