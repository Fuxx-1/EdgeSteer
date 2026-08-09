# Development and releases

[English](development.md) | [中文](../zh/development.md) | [Back to English README](README.md)

EdgeSteer is a Rust 2024 project with Rust 1.85 as its MSRV. Source, configuration, and documentation must keep one JSON schema and one fallback meaning. Add tests and an example before documenting new behavior.

## Repository layout

| Path | Content |
| --- | --- |
| `src/config.rs` | JSON schema, deserialization, and validation. |
| `src/dns.rs` | UDP/TCP listener, DoH/DoT/UDP/TCP upstreams, and response validation. |
| `src/plugins.rs` | Built-in response interceptors. |
| `src/optimizer.rs` | Cloudflare preferred-address probes. |
| `src/ranges.rs` | Cloudflare range loading and refresh. |
| `src/state.rs` | Runtime snapshots, DoH client cache, and concurrency control. |
| `src/watcher.rs` | Configuration hot reload. |
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
cargo run --release -- --config config.example.json --check-config
```

Tests do not depend on CI access to Cloudflare or Tencent. Network integration tests use a local fake UDP endpoint; key constraints for other transports are covered by configuration validation and pure unit tests, including response correlation, fallback order, keywords, interceptor rewriting, and reloads.

## Change rules

- Keep configuration objects strict JSON; new fields require updates to `config.example.json`, the configuration guides, and validation tests.
- Trigger fallback only for unacceptable network or protocol behavior. Do not silently replace valid NXDOMAIN, NODATA, SERVFAIL, or REFUSED with another provider's result.
- Accept only statically built-in plugin names; do not add dynamic-library, script, or shell execution paths.
- Read runtime state through a single `ArcSwap<RuntimeConfig>` snapshot so configuration and optimizer generations cannot mix.
- Listener rebinding needs an explicit lifecycle design. The current watcher leaves listener changes for restart; it must not create a second socket set.

## CI

`.github/workflows/ci.yml` runs rustfmt, Clippy, and tests on Linux, then release builds and tests on Linux, macOS, and Windows. CI uses the lockfile and Rust 1.85; reproduce those commands locally.

## Releases

`.github/workflows/release.yml` responds to `v*` tags and validates semantic versions:

- `v1.2.3` creates a normal GitHub Release.
- `v1.2.3-alpha.1`, `v1.2.3-beta.1`, and `v1.2.3-rc.1` create pre-releases.
- Invalid semantic-version tags fail before building.

The workflow builds Linux x86_64, Intel macOS, Apple Silicon macOS, and Windows x86_64 archives. Each release includes the binary, README, example configuration, and LICENSE.

Example:

```sh
git tag -a v0.3.0 -m "EdgeSteer v0.3.0"
git push origin v0.3.0
```

Before a release, ensure the worktree is clean, `cargo test --locked --all-targets` passes, the example configuration validates, and the release workflow builds on every target runner.

## Documentation maintenance

The root README is the default Chinese entry point. The topic files under `docs/zh/` and `docs/en/` correspond one-to-one and keep the same names. Update Chinese source facts first, then translate them to English and verify all relative links.
