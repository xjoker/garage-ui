#!/bin/sh
# Exercise the tag gate without building artifacts or images.
set -eu
cd "$(dirname "$0")/.."
python3 - <<'PY'
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest.mock

workflow = Path(".github/workflows/ci.yml").read_text()
gate = textwrap.dedent(workflow.split("          python3 - <<'PY_RELEASE_GATE'\n", 1)[1].split("          PY_RELEASE_GATE\n", 1)[0])
commit = "0123456789abcdef0123456789abcdef01234567"
version = "20261016.1"

with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    (root / "web").mkdir()
    commands = root / "commands"
    commands.mkdir()
    (commands / "git").write_text(f"#!/bin/sh\nprintf '%s\\n' '{commit}'\n")
    (commands / "git").chmod(0o755)

    def write_source(current_version=version, cargo_version=None, web_version=None, lock_version=None):
        expected = f"{current_version}.0"
        (root / "VERSION").write_text(current_version + "\n")
        (root / "Cargo.toml").write_text(
            "[workspace]\nmembers = []\n[workspace.package]\nversion = \"%s\"\n" % (cargo_version or expected)
        )
        locked = lock_version or expected
        (root / "Cargo.lock").write_text(
            "version = 4\n\n[[package]]\nname = \"garage-ui\"\nversion = \"%s\"\n\n[[package]]\nname = \"garage-loadgen\"\nversion = \"%s\"\n" % (locked, locked)
        )
        (root / "web/package.json").write_text(json.dumps({"version": web_version or expected}))

    def validate(tag=version, ref=None, workflow_commit=commit, **source):
        write_source(**source)
        output = root / "output"
        output.write_text("")
        env = dict(
            os.environ,
            PATH=f"{commands}:{os.environ['PATH']}",
            GITHUB_REF=ref or f"refs/tags/{tag}",
            GITHUB_REF_NAME=tag,
            GITHUB_SHA=workflow_commit,
            GITHUB_OUTPUT=str(output),
        )
        result = subprocess.run([sys.executable, "-c", gate], cwd=root, env=env, capture_output=True, text=True)
        return result, output.read_text()

    result, output = validate()
    assert result.returncode == 0, result.stderr
    assert output == f"version={version}\ncommit={commit}\n", output

    cases = [
        ("bad calendar date", dict(current_version="20260230.1", tag="20260230.1")),
        ("zero sequence", dict(current_version="20261016.0", tag="20261016.0")),
        ("leading zero sequence", dict(current_version="20261016.01", tag="20261016.01")),
        ("v prefix", dict(tag="v20261016.1")),
        ("mismatched tag", dict(tag="20261016.2")),
        ("mismatched Cargo manifest", dict(cargo_version="20261016.2.0")),
        ("mismatched web manifest", dict(web_version="20261016.2.0")),
        ("mismatched lockfile", dict(lock_version="20261016.2.0")),
        ("branch ref", dict(ref="refs/heads/dev")),
        ("mismatched commit", dict(workflow_commit="f" * 40)),
        ("abbreviated commit", dict(workflow_commit=commit[:7])),
    ]
    for name, kwargs in cases:
        result, output = validate(**kwargs)
        assert result.returncode != 0, f"{name} was accepted"
        assert not output, f"{name} reached release job outputs"

    # A later release must not retain the source template's previous image tag.
    release_script = Path("scripts/release.sh").read_text()
    render = release_script.split("<<'PY_DEPLOYMENT'\n", 1)[1].split("\nPY_DEPLOYMENT", 1)[0]
    (root / "deploy").mkdir()
    (root / "deploy/compose.external.yaml").write_text(Path("deploy/compose.external.yaml").read_text())
    for candidate in (version, "20261017.2"):
        destination = root / "compose.yaml"
        result = subprocess.run([sys.executable, "-c", render, candidate, str(destination)], cwd=root, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        assert "${GARAGE_UI_VERSION:-" + candidate + "}" in destination.read_text()

    # Publication must fail closed before touching Docker for an immutable
    # release or when GitHub cannot establish the existing release state.
    (root / "scripts").mkdir()
    (root / "scripts/publish.sh").write_text(Path("scripts/publish.sh").read_text())
    (root / "scripts/verify-image-tag-alias.sh").write_text(Path("scripts/verify-image-tag-alias.sh").read_text())
    alias_spec = importlib.util.spec_from_file_location("verify_image_tag_alias", "scripts/verify-image-tag-alias.py")
    alias_module = importlib.util.module_from_spec(alias_spec)
    alias_spec.loader.exec_module(alias_module)
    # SHA256SUMS must list every published file under its asset name. GitHub
    # renames an uploaded dotfile, so a dotfile name could never be verified.
    checksum_step = release_script.rsplit("names = [", 1)[1].split("]", 1)[0]
    checksummed = re.findall(r'"([^"]+)"', checksum_step)
    publish_files = re.search(r'^files="([^"]+)"', Path("scripts/publish.sh").read_text(), re.M).group(1).split()
    assert checksummed + ["SHA256SUMS"] == publish_files, (checksummed, publish_files)
    assert not [name for name in publish_files if name.startswith(".")], publish_files
    for name in checksummed:
        assert f'"$out/{name}"' in release_script, f"release.sh does not package {name}"
    # The uploaded assets are downloaded again and checked strictly before the draft is
    # published (publish.sh) and after publishing (the release workflow).
    publish_script = Path("scripts/publish.sh").read_text()
    verify_call = publish_script.find("scripts/verify-release-assets.sh")
    assert 0 < publish_script.find("gh release upload") < verify_call < publish_script.find("--draft=false"), "publish.sh must verify the uploaded assets before publishing"
    compare_call = publish_script.find('cmp -s "$assets/draft/SHA256SUMS" "$out/SHA256SUMS"')
    assert verify_call < compare_call < publish_script.find("--draft=false"), "publish.sh must compare the draft's SHA256SUMS with the prepared one before publishing"
    workflow = Path(".github/workflows/ci.yml").read_text()
    publish_step = workflow.find('run: scripts/publish.sh')
    verify_step = workflow.find('scripts/verify-release-assets.sh "$RUNNER_TEMP/published-assets" "$BUILD_VERSION"')
    assert 0 < publish_step < verify_step, "the release workflow must verify the published assets"
    verify_script = Path("scripts/verify-release-assets.sh").read_text()
    assert "sha256sum --check --strict SHA256SUMS" in verify_script and "--ignore-missing" not in verify_script.split("set -eu", 1)[1]
    def verify_assets(case, change):
        assets = root / "assets" / case
        assets.mkdir(parents=True)
        listed = ["garage-ui-linux-amd64", "compose.yaml", "default.env.example"]
        for name in listed:
            (assets / name).write_text(f"{name} fixture\n")
        (assets / "SHA256SUMS").write_text("".join(
            hashlib.sha256((assets / name).read_bytes()).hexdigest() + "  " + name + "\n" for name in listed
        ))
        change(assets)
        return subprocess.run(["sh", "scripts/verify-release-assets.sh", str(assets)], capture_output=True, text=True)
    def rename_template(assets):
        sums = assets / "SHA256SUMS"
        sums.write_text(sums.read_text().replace("default.env.example", ".env.example"))
    ok = verify_assets("ok", lambda assets: None)
    assert ok.returncode == 0, ok.stderr
    for case, change, expected in [
        ("renamed", rename_template, "listed in SHA256SUMS, not an asset: .env.example"),
        ("extra", lambda assets: (assets / "notes.txt").write_text("x\n"), "asset not listed in SHA256SUMS: notes.txt"),
        ("hidden", lambda assets: (assets / ".notes").write_text("x\n"), "asset not listed in SHA256SUMS: .notes"),
        ("directory", lambda assets: (assets / "extra").mkdir(), "asset not listed in SHA256SUMS: extra"),
        ("missing", lambda assets: (assets / "compose.yaml").unlink(), "listed in SHA256SUMS, not an asset: compose.yaml"),
        ("changed", lambda assets: (assets / "compose.yaml").write_text("changed\n"), "FAILED"),
    ]:
        result = verify_assets(case, change)
        assert result.returncode != 0, f"verify-release-assets accepted {case}"
        assert expected in result.stdout + result.stderr, (case, result.stdout, result.stderr)
    artifacts = root / "artifacts"
    artifacts.mkdir()
    names = ["garage-ui-linux-amd64", "garage-ui-linux-arm64", "sbom.spdx.json", "compose.yaml", "default.env.example"]
    for name in names:
        (artifacts / name).write_text("synthetic release fixture\n")
    (artifacts / "SHA256SUMS").write_text("".join(
        hashlib.sha256((artifacts / name).read_bytes()).hexdigest() + "  " + name + "\n" for name in names
    ))
    (commands / "docker").write_text('#!/bin/sh\ntouch "$DOCKER_PROBE"\nexit 9\n')
    (commands / "docker").chmod(0o755)
    for response in (f"printf '{version}\\tfalse\\n'", f"printf '{version}\\ttrue\\n{version}\\ttrue\\n'", f"printf '{version}\\ttrue\\n{version}\\tfalse\\n'", "exit 7"):
        (commands / "gh").write_text("#!/bin/sh\n" + response + "\n")
        (commands / "gh").chmod(0o755)
        probe = root / "docker-called"
        env = dict(os.environ, PATH=f"{commands}:{os.environ['PATH']}", DOCKER_PROBE=str(probe))
        result = subprocess.run(["sh", "scripts/publish.sh", str(artifacts)], cwd=root, env=env, capture_output=True, text=True)
        if response == "exit 7":
            assert result.returncode == 7, result.stderr
        elif "true" in response and "false" not in response:
            assert result.returncode != 0 and "Several draft releases use tag" in result.stderr, result.stderr
        else:
            assert "already published" in result.stderr, result.stderr
        assert not probe.exists(), "Publication reached Docker before confirming a writable draft"

    # The alias gate reads each tag's manifest with `imagetools inspect --raw` and hashes it.
    # A fake docker serves scripted registry answers; a fake clock replaces the waits. The
    # gate must pass only on equal digests read in one attempt and fail on a mismatch or on
    # a digest it cannot read, with the reason in the log.
    alias_source = Path("scripts/verify-image-tag-alias.py").read_text()
    assert '"--raw"' in alias_source and "--format" not in alias_source.split('"""', 2)[2], "read the raw manifest, not a Buildx template"
    alias_commands = root / "alias-commands"
    alias_commands.mkdir()
    (alias_commands / "docker").write_text(textwrap.dedent(f"""\
        #!{sys.executable}
        import json, os, sys, time
        state = os.environ["ALIAS_FIXTURE"]
        with open(state) as handle:
            fixture = json.load(handle)
        reference = sys.argv[-1]
        assert sys.argv[1:-1] == ["buildx", "imagetools", "inspect", "--raw"], sys.argv
        queue = fixture[reference.rsplit(":", 1)[1]]
        answer = queue.pop(0) if len(queue) > 1 else queue[0]
        with open(state, "w") as handle:
            json.dump(fixture, handle)
        if "spawn" in answer:
            import subprocess
            helper = subprocess.Popen(["sleep", "30"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            with open(answer["spawn"], "w") as handle:
                handle.write(str(helper.pid))
        time.sleep(answer.get("sleep", 0))
        sys.stdout.write(answer.get("stdout", ""))
        sys.stderr.write(answer.get("stderr", ""))
        sys.exit(answer.get("code", 0))
    """))
    (alias_commands / "docker").chmod(0o755)

    def index(*platform_digests):
        return json.dumps({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                {"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": "sha256:" + d * 64, "size": 1,
                 "platform": {"architecture": arch, "os": "linux"}}
                for d, arch in zip(platform_digests, ("amd64", "arm64"))
            ],
        })
    released, stale = index("a", "b"), index("c", "d")
    released_digest = "sha256:" + hashlib.sha256(released.encode()).hexdigest()
    stale_digest = "sha256:" + hashlib.sha256(stale.encode()).hexdigest()
    ok = {"stdout": released}

    class FakeClock:
        def __init__(self):
            self.now = 0.0
            self.sleeps = []
        def monotonic(self):
            return self.now
        def sleep(self, seconds):
            self.sleeps.append(seconds)
            self.now += seconds

    def run_alias(fixture, timeout=30, attempt_timeout=20, extra_path=True):
        state = root / "alias-fixture.json"
        state.write_text(json.dumps(fixture))
        clock = FakeClock()
        out, err = io.StringIO(), io.StringIO()
        path = f"{alias_commands}:{os.environ['PATH']}" if extra_path else str(root / "empty-path")
        with unittest.mock.patch.dict(os.environ, {"ALIAS_FIXTURE": str(state), "PATH": path}), \
                unittest.mock.patch.object(alias_module, "time", clock), \
                unittest.mock.patch.object(alias_module, "ATTEMPT_TIMEOUT", attempt_timeout), \
                contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            passed = alias_module.verify_alias("ghcr.io/example/image", "v1", "latest", timeout=timeout)
        return passed, out.getvalue(), err.getvalue(), clock.sleeps

    # Equal digests pass on the first attempt and name the digest of the bytes served.
    passed, out, err, sleeps = run_alias({"v1": [ok], "latest": [ok]})
    assert passed and released_digest in out and not err and not sleeps, (out, err)

    # A different digest keeps polling, then fails; both digests are in the failure.
    passed, out, err, sleeps = run_alias({"v1": [ok], "latest": [{"stdout": stale}]}, timeout=10)
    assert not passed and "did not converge" in err and f"version={released_digest}, alias={stale_digest}" in err, err
    assert "unavailable" not in err and sleeps == [3, 3, 3, 1], (err, sleeps)

    # A stale alias that converges within the timeout passes.
    passed, out, err, sleeps = run_alias({"v1": [ok], "latest": [{"stdout": stale}, ok]})
    assert passed and f"alias={stale_digest}" in err and sleeps == [3], (err, sleeps)

    # A digest that cannot be read fails and says why. Credentials in error output are not logged.
    denied = {"code": 1, "stderr": "ERROR: failed to authorize: Authorization: Bearer ghp_" + "x" * 36 + ": 401 Unauthorized\n"}
    passed, out, err, sleeps = run_alias({"v1": [denied], "latest": [ok]})
    assert not passed and "unavailable (permanent: exit status 1:" in err and "401 Unauthorized" in err, err
    assert "ghp_" not in err and "Not retrying" in err and not sleeps, (err, sleeps)
    # Buildx v0.25.0 printed this summary for `--format '{{.Manifest.Digest}}'`; it is not a digest.
    summary = {"stdout": f"Name:      ghcr.io/example/image:v1\nDigest:    {released_digest}\n"}
    passed, out, err, sleeps = run_alias({"v1": [summary], "latest": [summary]}, timeout=6)
    assert not passed and not out and "unavailable (permanent: output is not a manifest" in err and not sleeps, (err, sleeps)
    # Valid JSON that is not a manifest, such as a registry error document, is not a digest either.
    errors_document = {"stdout": json.dumps({"errors": [{"code": "MANIFEST_UNKNOWN", "message": "manifest unknown"}]})}
    passed, out, err, sleeps = run_alias({"v1": [errors_document], "latest": [errors_document]})
    assert not passed and not out and "unavailable (permanent: unsupported manifest media type None)" in err and not sleeps, err
    # Only the words of a 401/403 answer are permanent; a bare number elsewhere is not.
    short_read = {"code": 1, "stderr": "ERROR: unexpected EOF after read 401 bytes\n"}
    passed, out, err, sleeps = run_alias({"v1": [short_read, ok], "latest": [ok]})
    assert passed and "unavailable (transient: exit status 1: ERROR: unexpected EOF after read 401 bytes)" in err, err
    unavailable = {"code": 1, "stderr": "ERROR: unexpected status from HEAD request: 503 Service Unavailable\n"}
    passed, out, err, sleeps = run_alias({"v1": [ok], "latest": [unavailable]}, timeout=20)
    assert not passed and err.rstrip().endswith("alias=unavailable (transient: exit status 1: ERROR: unexpected status from HEAD request: 503 Service Unavailable))"), err
    assert sleeps == [3, 6, 11], sleeps
    helper_pid = root / "alias-helper.pid"
    passed, out, err, sleeps = run_alias({"v1": [ok], "latest": [{"sleep": 5, "stdout": released, "spawn": str(helper_pid)}]}, timeout=1, attempt_timeout=0.2)
    assert not passed and "no response within 0.2 seconds" in err, err
    # The timeout stops the whole process group, including a helper the command started.
    pid = int(helper_pid.read_text())
    for _ in range(10):
        try:
            state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
        except FileNotFoundError:
            state = "gone"
        if state in ("gone", "Z", "X"):
            break
        time.sleep(0.05)
    if state not in ("gone", "Z", "X"):
        os.kill(pid, 9)
    assert state in ("gone", "Z", "X"), f"helper process {pid} survived the timeout ({state})"
    (root / "empty-path").mkdir()
    passed, out, err, sleeps = run_alias({"v1": [ok], "latest": [ok]}, extra_path=False)
    assert not passed and "docker is not installed" in err and not sleeps, err

    # A transient error followed by a good read passes, and the failed attempt is logged.
    passed, out, err, sleeps = run_alias({"v1": [unavailable, ok], "latest": [ok]})
    assert passed and "Attempt 1:" in err and "503 Service Unavailable" in err and released_digest in out and sleeps == [3], (err, sleeps)

    # Each redaction rule has its own cases. Every sample is caught by its rule and by no other
    # rule on its own, so removing any one rule fails its cases.
    redaction_cases = {
        "bearer-basic": [
            ("Authorization: Bearer opaqueBearerValue123", "opaqueBearerValue123"),
            ("Authorization: Basic dXNlcjpvcGFxdWVCYXNpYw", "dXNlcjpvcGFxdWVCYXNpYw"),
        ],
        "key-value": [
            ("GET /token?scope=x&token=opaqueQueryToken1", "opaqueQueryToken1"),
            ("access_token=opaqueAccessToken1&expires=1", "opaqueAccessToken1"),
            ("refresh_token=opaqueRefresh1", "opaqueRefresh1"),
            ("id_token=opaqueIdToken1", "opaqueIdToken1"),
            ("client_secret=opaqueClientSecret1", "opaqueClientSecret1"),
            ("login with password=opaquePassword1", "opaquePassword1"),
            ("url-encoded token%3DopaqueEncoded1", "opaqueEncoded1"),
        ],
        "json-field": [
            ('{"token":"opaqueJsonToken1","expires_in":300}', "opaqueJsonToken1"),
            ('{"access_token": "opaqueJsonToken2"}', "opaqueJsonToken2"),
            ('{"refresh_token" : "opaque json token 3"}', "opaque json token 3"),
        ],
        "password-colon": [
            ("password: opaqueColonPass1", "opaqueColonPass1"),
            ("Password:opaqueColonPass2", "opaqueColonPass2"),
        ],
        "url-userinfo": [
            ("pull https://robot:opaqueUserinfo1@ghcr.io/v2/x", "opaqueUserinfo1"),
        ],
        "jwt": [
            ("expired eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ4In0.c2lnbmF0dXJl", "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ4In0.c2lnbmF0dXJl"),
        ],
        "github-token": [
            ("credential ghp_" + "A1b2C3d4" * 5, "ghp_" + "A1b2C3d4" * 5),
            ("credential github_pat_" + "Z9y8X7w6" * 4, "github_pat_" + "Z9y8X7w6" * 4),
        ],
    }
    for name, cases in redaction_cases.items():
        for sample, secret in cases:
            redacted = alias_module.summarize(sample)
            assert secret not in redacted and "[redacted]" in redacted, (name, redacted)
    rules = {name: (pattern, replacement) for name, pattern, replacement in alias_module.SECRETS}
    assert set(rules) == set(redaction_cases), (sorted(rules), sorted(redaction_cases))
    for name, cases in redaction_cases.items():
        for sample, secret in cases:
            pattern, replacement = rules[name]
            assert secret not in pattern.sub(replacement, sample), (name, sample)
            for other, (other_pattern, other_replacement) in rules.items():
                if other != name:
                    assert secret in other_pattern.sub(other_replacement, sample), (name, other, sample)
    # Diagnostic text without credentials is kept.
    plain = "ERROR: failed to fetch anonymous token: unexpected status: 503 Service Unavailable sha256:" + "a" * 64
    assert alias_module.summarize(plain) == plain, alias_module.summarize(plain)

    assert "needs: gate" in workflow, "release must not run before the tag gate"
    assert "pull_request:" not in workflow and "branches:" not in workflow, "ordinary pushes and PRs must not build"
print("Release tag gate checks passed")
PY
