# Architecture

[English](architecture.md) | [中文](../zh/architecture.md) | [Back to English README](README.md)

EdgeSteer is not a string heuristic that replaces addresses when a domain looks like Cloudflare. It sends each DNS request through a validated resolver fallback chain, then applies a constrained response interceptor. This covers sites hosted on Cloudflare without Cloudflare-related names and avoids rewriting non-Cloudflare answers.

## System shape

```mermaid
flowchart LR
    C["DNS client request"] --> P["Parse and validate"]
    P --> R["Select start by keyword"]
    R --> I["Interceptor chain"]
    I --> D["Cloudflare DoH"]
    D --> T["Tencent DoH"]
    T --> L["Dynamic system DNS upstreams"]
    I -. "successful response" .-> W["Validate and rewrite"]
    D -. "network/protocol failure" .-> T
    T -. "network/protocol failure" .-> L
    W --> O["Return to client"]
    L --> O
```

Every layer is a node and its `fallback` points to the next node. The JSON describes a graph, but execution follows one validated, acyclic successor chain. Keyword rules choose the start; they do not duplicate a query across resolvers.

## Component responsibilities

| Module | Responsibility |
| --- | --- |
| `src/dns.rs` | UDP/TCP listeners, request parsing, request deadline, layer execution, upstream correlation checks, and response encoding. |
| `src/config.rs` | Strict JSON parsing, field constraints, layer/plugin references, acyclic fallback validation, keyword matching, and DoH/DoT validation. |
| `src/local_dns.rs` | Discovers local upstreams from system network configuration, filters loopback/self addresses, refreshes periodically, and rediscovers after local failures. |
| `src/plugins.rs` | Statically built-in interceptors. `cloudflare_preferred` rewrites A, AAAA, HTTPS/SVCB hints and clears DNSSEC state. |
| `src/optimizer.rs` | TCP, TLS, and HTTP probes for Cloudflare candidates; chooses the fastest IPv4 and IPv6 independently. |
| `src/ranges.rs` | Built-in Cloudflare ranges at startup and periodic refresh from official `ips-v4` and `ips-v6` endpoints; failed refreshes keep the current list. |
| `src/state.rs` | `ArcSwap` runtime snapshot, DoH client cache, and concurrency control. |
| `src/watcher.rs` | Configuration watcher with approximately 250 ms debounce and atomic replacement after validation. |

## Request lifecycle

1. The listener receives a UDP datagram or TCP length-prefixed frame. UDP work is limited to 128 in-flight permits; excess bursts are dropped so clients can retry.
2. Only DNS queries are accepted. Malformed packets, DNS responses received on the listener, and unparseable requests are discarded.
3. The request reads one `RuntimeConfig` snapshot and the current Cloudflare ranges. A single-question request provides its QNAME; a multi-question request uses `entry` directly.
4. For one question, the first matching `match` in `layers` declaration order selects the start. Without a match, execution starts at `entry`. A selected layer follows only its own `fallback` chain.
5. Network layers use both a global request deadline and a per-layer timeout. `local` selects cached system-DNS addresses in order. A truncated UDP response is retried over TCP against the same endpoint.
6. An upstream response must match transaction ID, QR/message type, opcode, and question. DoH also requires a successful HTTP status, a non-empty body, and `application/dns-message`.
7. After a network layer succeeds, interceptors run in reverse entry order. `cloudflare_preferred` rewrites only when all relevant addresses are in the active Cloudflare ranges; no rewrite is a successful no-op.
8. If every network layer fails, EdgeSteer creates a `SERVFAIL` response containing the original questions rather than forwarding malformed or mismatched bytes.

## Fallback and response semantics

Fallback means that the current layer could not provide an acceptable DNS wire response: connection timeout, TLS or HTTP failure, wrong Content-Type, empty body, malformed DNS, or failed correlation checks. A valid DNS response ends the chain, including `NXDOMAIN`, NODATA, `SERVFAIL`, and `REFUSED`. This avoids changing valid answers because providers disagree and avoids sending the same query to more providers.

An interceptor does not synthesize a complete DNS answer. It can only change allowed records after a downstream resolver returns a valid response. Rewriting clears `AD`, EDNS `DO`, and RRSIG records so modified data is not presented as DNSSEC-authenticated.

## Cloudflare detection and optimization

Detection uses numeric addresses in the response and IP hints in HTTPS/SVCB records. They must belong to the active Cloudflare ranges. The ranges start from a built-in list and are refreshed from the official lists; a failed refresh never clears the active list.

The optimizer samples configured IP/CIDR candidates. A candidate must pass TCP connect, TLS handshake, and the HTTP probe using `test_host` and `test_path`; the response must be 2xx with `server: cloudflare`. IPv4 and IPv6 are selected independently, and a failed family retains its last good value.

## Reload and consistency

The watcher fully parses and validates new JSON before replacing the runtime snapshot. In-flight requests keep their old snapshot and later requests see the new one, so configuration and optimizer state cannot be mixed across generations. Layer changes clear cached DoH clients; when a `local` layer exists, its refresh loop rereads system DNS at the new shortest `refresh_secs` interval.

Listener address and `allow_remote` changes cannot rebind sockets dynamically. The file can be accepted, but the process keeps the existing listener and logs that a restart is required; other valid settings still reload.

## Security boundaries

- JSON selects only statically compiled built-in plugins; it cannot load libraries, scripts, or shell commands.
- For DoH, `address` is a fixed numeric bootstrap. The URL hostname supplies TLS SNI, HTTP Host, and certificate validation; proxy environment settings are disabled and redirects are not followed.
- DoT requires an explicit `server_name` and validates TLS with the bundled WebPKI roots.
- `local` does not call the system resolver. It reads numeric DNS addresses from system network configuration and filters loopback, unspecified, multicast, IPv6 link-local, listener addresses, and macOS virtual-tunnel services. If system configuration already points to EdgeSteer, it fails rather than looping and cannot restore an overwritten underlay DNS.
- Local queries use native UDP/TCP sockets, which does not mean bypassing sing-box TUN or transparent interception; outer proxy rules must route the underlay DNS correctly.
- The default listener binds to loopback. If `allow_remote` is enabled, provide network access control, rate limiting, and monitoring.
