# Contributing to Sesame

Thanks for helping open servers for the Laravel community.

## Setup

1. Install Rust (`rustup`) and, on macOS, Xcode (GPUI needs the Metal toolchain).
2. `cargo run -p sesame-app` starts the app; `cargo test --workspace` runs the tests.
3. `cargo test -p sesame-app --test screenshots` renders every screen to `target/screenshots/` — attach before/after images to UI pull requests.

## Ground rules

- **Discovery is read-only.** Anything that changes a server is an operation with a `Risk` level and goes through confirmation.
- **No secrets in argv, logs or SQLite.** Pass them over stdin; store them in the encrypted vault.
- **Logic stays out of the UI crate.** `sesame-app` is the only crate that depends on GPUI, so mobile, MCP/REST and the cloud reuse everything else.
- New probe output → add a fixture under `crates/sesame-discovery/fixtures/` and a test.
- Every user-facing string goes through `t!()` with English and Russian translations.

## Commits and releases

- Conventional, imperative commit subjects (`Add Redis recipe`, `Fix nginx include parsing`).
- Add a line under `## [Unreleased]` in `CHANGELOG.md` for anything users notice.
- Versions follow SemVer: `0.1.0-alpha.N` → `0.1.0-beta.N` → `0.1.0`. A `v*` tag builds and publishes the release.
