# Development and releases

[English](development.md) | [中文](../zh/development.md) | [Back to English README](README.md)

EdgeSteer is a Rust 2024 project with Rust 1.85 as its MSRV. Source, configuration, and documentation must keep one JSON schema and one fallback meaning. Add tests and an example before documenting new behavior.

## Repository layout

| Path | Content |
| --- | --- |
| `src/config.rs` | JSON schema, deserialization, and validation. |
| `src/dns.rs` | UDP/TCP listener, DoH/DoT/UDP/TCP upstreams, and response validation. |
| `src/local_dns.rs` | System network DNS discovery and refresh for macOS, Linux, and Windows. |
| `src/plugins.rs` | Built-in response interceptors. |
| `src/optimizer.rs` | Cloudflare preferred-address probes. |
| `src/ranges.rs` | Cloudflare range loading and refresh. |
| `src/state.rs` | Runtime snapshots, DoH client cache, and concurrency control. |
| `src/watcher.rs` | Configuration hot reload. |
| `src/agent.rs` | Resident menu-bar Agent, DNS-engine ownership, and authenticated loopback control. |
| `src/ui.rs`, `src/tray.rs` | Disposable Iced settings process and native menu-bar presentation. |
| `src/main.rs`, `src/lib.rs` | CLI, logging, and process lifecycle. |
| `config.example.json` | Copyable configuration example. |
| `.github/workflows/` | CI and tag-triggered releases. |

## Local quality gate

Run before committing:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

Validate only the configuration:

```sh
cp config.example.json "$HOME/edgesteer.json"
cargo run --release -- --check-config
```

Tests do not depend on CI access to Cloudflare or Tencent. Network integration tests use a local fake UDP endpoint; key constraints for other transports are covered by configuration validation and pure unit tests, including response correlation, fallback order, keywords, SRS domain rules, dynamic local caching, interceptor rewriting, and reloads.

## Change rules

- Keep configuration objects strict JSON; new fields require updates to `config.example.json`, the configuration guides, and validation tests.
- Trigger fallback only for unacceptable network or protocol behavior. Do not silently replace valid NXDOMAIN, NODATA, SERVFAIL, or REFUSED with another provider's result.
- Accept only statically built-in plugin names; do not add dynamic-library, script, or shell execution paths.
- Read runtime state through a single `ArcSwap<RuntimeConfig>` snapshot so configuration and optimizer generations cannot mix.
- Listener rebinding needs an explicit lifecycle design. The current watcher leaves listener changes for restart; it must not create a second socket set.

## CI

`.github/workflows/ci.yml` runs rustfmt, Clippy, and tests on Linux x86_64, then runs release builds and tests on native x86_64 and ARM64 runners for Linux, macOS, and Windows. All six native jobs exercise the same JSON schema, DNS fallback, plugin, and optimizer tests. CI uses the lockfile and Rust 1.85; reproduce those commands locally.

## Releases

`.github/workflows/release.yml` responds to `v*` tags and validates semantic versions:

- `v1.2.3` creates a normal GitHub Release.
- `v1.2.3-alpha.1`, `v1.2.3-beta.1`, and `v1.2.3-rc.1` create pre-releases.
- Invalid semantic-version tags fail before building.

The workflow builds x86_64 and ARM64 assets for Linux, macOS, and Windows. Linux and Windows publish archives containing the CLI and UI binaries. macOS publishes architecture-specific unsigned `.dmg` images containing an `EdgeSteer.app` that can be dragged to Applications; the App bundle manages the DNS engine (using a hidden elevated helper for port 53 when needed) and contains no standalone command-line service.

Example:

```sh
git tag -a v0.4.0 -m "EdgeSteer v0.4.0"
git push origin v0.4.0
```

Before a release, ensure the worktree is clean, `cargo test --locked --all-targets` passes, the example configuration validates, and the release workflow builds on every target runner.

## Documentation maintenance

The root README is the default Chinese entry point. The topic files under `docs/zh/` and `docs/en/` correspond one-to-one and keep the same names. Update Chinese source facts first, then translate them to English and verify all relative links.
