# EdgeSteer

[![CI](https://github.com/Fuxx-1/EdgeSteer/actions/workflows/ci.yml/badge.svg)](https://github.com/Fuxx-1/EdgeSteer/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](../../LICENSE)

[English](README.md) | [中文](../../README.md)

EdgeSteer is a local Rust DNS steering proxy for macOS, Linux, and Windows. It uses JSON to describe a multi-layer upstream fallback chain and a built-in interceptor that can replace verified Cloudflare addresses with the current preferred IP.

It identifies Cloudflare from DNS response data, not from spelling. A site can be hosted on Cloudflare even when its domain and CNAME contain neither `cf` nor `cloudflare`; A, AAAA, HTTPS, and SVCB address data is checked against Cloudflare's published ranges. Mixed or unverified answers are left unchanged.

## How it works

The sample routes by keyword and `sing-geosite` rule set:

```mermaid
flowchart TB
    Client["DNS client"] --> Match["domain match"]
    Match -->|"b2c / mi / local"| LocalKeyword["dynamic local DNS"]
    Match -->|"geosite-cn"| CN["CF preferred interceptor"]
    Match -->|"geosite-geolocation-!cn"| Overseas["CF preferred interceptor"]
    Match -->|"unmatched / multi-question"| Default["CF preferred interceptor"]
    CN --> Tencent["Tencent DoH"]
    Default --> Tencent
    Tencent --> CF["Cloudflare DoH"]
    Overseas --> CF
    CF --> LocalFallback["dynamic local DNS"]
```

The concrete selection order is:

- `b2c`, `mi`, or `local` keyword → dynamic local DNS;
- `geosite-cn` → preferred interceptor → Tencent DoH → Cloudflare DoH → dynamic local DNS;
- `geosite-geolocation-!cn` → preferred interceptor → Cloudflare DoH → dynamic local DNS;
- a domain not covered by either rule set (and multi-question packets) → preferred interceptor → Tencent DoH → Cloudflare DoH → dynamic local DNS.

`geosite-geolocation-!cn` is not the complement of all Chinese domains: it only covers the overseas domains present in that rule set. Unknown domains therefore retain the full default fallback chain. Every branch that can use a preferred address begins with the interceptor. It lets the downstream upstream return a complete DNS response, then rewrites A, AAAA, and HTTPS/SVCB hints only when the relevant addresses are all verified as Cloudflare. Rewriting clears DNSSEC `AD`, `DO`, and RRSIG state.

The built-in optimizer probes Cloudflare addresses with TCP, TLS, and HTTP. A candidate must return a 2xx response with `server: cloudflare`; the fastest IPv4 and IPv6 are selected independently. This is a reachability and latency selector, not a bandwidth benchmark.

## Quick start

Rust 1.85 or newer is required. Release archives target Linux x86_64, Intel macOS, Apple Silicon macOS, and Windows x86_64.

```sh
git clone https://github.com/Fuxx-1/EdgeSteer.git
cd EdgeSteer
cp config.example.json edgesteer.json
cargo build --locked --release
./target/release/edgesteer --config edgesteer.json --check-config
RUST_LOG=info ./target/release/edgesteer --config edgesteer.json
```

PowerShell:

```powershell
Copy-Item config.example.json edgesteer.json
cargo build --locked --release
.\target\release\edgesteer.exe --config edgesteer.json --check-config
$env:RUST_LOG = "info"
.\target\release\edgesteer.exe --config edgesteer.json
```

Use a high port for the first test:

```json
{
  "listener": { "address": "127.0.0.1:53535", "allow_remote": false }
}
```

```sh
dig @127.0.0.1 -p 53535 www.cloudflare.com A +short
```

See the [configuration guide](configuration.md) for all fields, DoH/DoT constraints, keyword and sing-box SRS domain-rule matching, and plugin examples.

## Documentation

| Topic | English | 中文 |
| --- | --- | --- |
| Project overview and quick start | This page | [README](../../README.md) |
| Architecture and request lifecycle | [architecture.md](architecture.md) | [architecture.md](../zh/architecture.md) |
| JSON configuration, matching, and plugins | [configuration.md](configuration.md) | [configuration.md](../zh/configuration.md) |
| Installation, operations, reload, and troubleshooting | [operations.md](operations.md) | [operations.md](../zh/operations.md) |
| Development, tests, CI, and releases | [development.md](development.md) | [development.md](../zh/development.md) |

## Important boundaries

- The configuration is strict JSON. Unknown fields are rejected; `entry`, layer, fallback, and plugin references must exist, and fallback chains cannot contain cycles.
- `local` dynamically reads real network DNS and never calls the operating-system resolver. It filters loopback, listener, and virtual-tunnel DNS. When a macOS physical service points at the local listener, EdgeSteer reads that service's current DHCP option 6 DNS servers instead of saving or replaying old addresses.
- Native UDP/TCP queries do not bypass sing-box TUN or transparent DNS interception. Routing direct access to the underlay DNS belongs in the outer proxy configuration.
- Only network, TLS, HTTP, empty-body, malformed-DNS, or response-correlation failures enter fallback. Valid NXDOMAIN, NODATA, SERVFAIL, and REFUSED responses are returned immediately.
- Plugins are statically compiled built-ins. JSON cannot load a dynamic library, script, or external command.
- The default listener binds only to loopback. Enabling `allow_remote: true` makes it a LAN DNS service and requires your own access control and abuse protection.

## License

GPL-3.0-only; see [LICENSE](../../LICENSE). EdgeSteer is independent from and not endorsed by Cloudflare.
