#!/bin/sh
# Exercise the tag gate without building artifacts or images.
set -eu
cd "$(dirname "$0")/.."
python3 - <<'PY'
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
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

    # GHCR can serve a stale manifest after publishing a tag alias. Verify the
    # retry and timeout behavior without waiting on a real registry.
    digest_responses = iter(["sha256:version", "sha256:old", "sha256:version", "sha256:version"])
    with unittest.mock.patch.object(alias_module, "inspect_digest", side_effect=lambda *_: next(digest_responses)):
        with unittest.mock.patch.object(alias_module.time, "monotonic", side_effect=[0, 0, 0, 3]):
            with unittest.mock.patch.object(alias_module.time, "sleep") as sleep:
                assert alias_module.verify_alias("ghcr.io/example/image", "v1", "latest", timeout=10)
                sleep.assert_called_once_with(3)

    times = iter([0, 0, 0, 2, 2, 4])
    with unittest.mock.patch.object(
        alias_module,
        "inspect_digest",
        side_effect=lambda reference, *_: "sha256:version" if reference.endswith(":v1") else "sha256:stale",
    ):
        with unittest.mock.patch.object(alias_module.time, "monotonic", side_effect=lambda: next(times, 4)):
            with unittest.mock.patch.object(alias_module.time, "sleep") as sleep:
                assert not alias_module.verify_alias("ghcr.io/example/image", "v1", "latest", timeout=4, poll_interval=2)
                assert sleep.call_count == 2

    assert "needs: gate" in workflow, "release must not run before the tag gate"
    assert "pull_request:" not in workflow and "branches:" not in workflow, "ordinary pushes and PRs must not build"
print("Release tag gate checks passed")
PY
