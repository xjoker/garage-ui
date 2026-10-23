# Development guide

This repository ships a Rust service with an embedded React/Vite interface. Use this page as a short entry point; [Contributing](CONTRIBUTING.md) owns the complete local workflow and [docs/engineering.md](docs/engineering.md) owns release rules.

## Local work

Install the Rust toolchain pinned in `rust-toolchain.toml`, Bun from `web/package.json`, Docker for integration checks, and Python 3. From the repository root:

```sh
scripts/build.sh
```

The command installs locked frontend dependencies, builds `web/dist`, and builds the Rust binary. The release binary embeds that frontend bundle.

For a focused change, run the affected Rust or frontend test and then the relevant formatter, lint, or build check. UI work also needs a real-browser check at desktop and narrow widths. Do not use production Garage data as a test fixture.

## Release path

Releases are produced by GitHub Actions from a matching `YYYYMMDD.N` tag. The version in `VERSION`, the Cargo workspace, and `web/package.json` must agree. The workflow validates the tag, runs release checks, builds amd64 and arm64 images, publishes GHCR images and release assets, then verifies the published artifacts.

Local build scripts are for development and release diagnosis; they do not replace the CI release path. Do not put tokens, passwords, private hostnames, or machine paths in commits, documentation, screenshots, or evidence.
