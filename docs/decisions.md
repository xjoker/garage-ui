# Implementation decisions

## Runtime and storage

Garage UI is one Rust service with an embedded React interface. Runtime accounts, sessions, audit records, selected bucket keys, and the session key live in its dedicated data directory; Garage stores the actual objects and cluster state.

The current stack is defined by [Cargo.toml](../crates/garage-ui/Cargo.toml), [Cargo.lock](../Cargo.lock), [web/package.json](../web/package.json), and [web/bun.lock](../web/bun.lock). Do not treat older dependency snapshots as installation instructions.

- Rust 1.99.0 is pinned in `rust-toolchain.toml`.
- Tokio, axum, and Hyper serve the HTTP API. Reqwest and the AWS S3 SDK handle outgoing Garage requests through rustls.
- SQLx uses bundled SQLite. Account passwords use bcrypt; session cookies are stored as digests.
- RustEmbed includes the frontend in release binaries. JSON tracing goes to stdout/stderr.
- Linux release binaries target musl on amd64 and arm64. The runtime image is distroless and runs as UID/GID 65532.

## Frontend

Bun 1.4.2 installs dependencies from the frozen lockfile and runs Vite, TypeScript, ESLint, and Vitest. React 18, Radix Themes and `radix-ui` provide controls; TanStack Query manages server data, and react-hook-form with zod validates forms. Theme and navigation state stay in components. File inputs remain native.

Use the existing Radix color scales, typography, and shared controls. The interface supports light, dark, and system themes, English and Simplified Chinese, keyboard interaction, and narrow viewports. The frontend ships inside the Rust service and needs no separate runtime or browser CDN.

## Garage compatibility

The supported stable baseline is Garage v2.3.0; compatibility is exercised with v2.3.0 and v2.4.1. Features unavailable in a connected version use capability checks. See the [support assessment](garage-support.md) for the exact boundary and dated upstream evidence.

Integration checks use disposable Garage containers and data. Application code never installs or upgrades an operator's Garage cluster. Compose examples can start a separate single-node Garage for evaluation.

## Release versions

The root `VERSION` is the source of truth: `YYYYMMDD.N`, with `.1` for the first release that day and a daily sequence reset. Read the current date and existing tags before choosing a version. Cargo workspace/lockfile and the web manifest use the same value followed by `.0`; that suffix satisfies SemVer. Git tags, image tags, and GitHub release names have no `v` prefix. Helm's chart version remains SemVer, while `appVersion` and its image default follow `VERSION`.

## Build and release identity

Official releases are built and published by GitHub Actions from matching version tags. The workflow verifies frontend and Rust tests, release scripts, and source/history secrets before native amd64 and arm64 image builds. It validates image health identity, public registry access, and downloaded release checksums before completing publication.

Local builds default to `<VERSION>-local`. Ordinary branch pushes do not allocate a CI development version. `BUILD_VERSION` and `GIT_COMMIT` identify the validated release tag and exact source commit; `/healthz` exposes both. Published releases are immutable. Local scripts support development and diagnosis; they do not replace the Actions publication path.

Artifacts include multi-architecture images, static Linux binaries, deployment files, an SPDX SBOM, and SHA-256 checksums. Signing and build-provenance attestations remain deferred; checksums verify integrity, not publisher identity.

## Concurrent contributions

Assign non-overlapping file ownership and coordinate shared ports, Garage fixtures, and test databases. The coordinating contributor stages, commits, and closes full-suite tests. Preserve other contributors' edits and keep personal paths, live credentials, and raw local receipts out of public changes. See [Contributing](../CONTRIBUTING.md) for commands and review requirements.
