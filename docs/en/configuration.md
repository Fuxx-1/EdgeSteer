# Configuration

[English](configuration.md) | [中文](../zh/configuration.md) | [Back to English README](README.md)

EdgeSteer uses strict JSON. Objects marked with `deny_unknown_fields` reject misspelled and unimplemented fields. Use `--check-config` before starting a listener.

## Minimal complete chain

This example represents “preferred interceptor -> Cloudflare DoH -> Tencent DoH -> local”:

```json
{
  "listener": {
    "address": "127.0.0.1:53535",
    "allow_remote": false
  },
  "cloudflare": {
    "range_refresh_secs": 86400
  },
  "request_timeout_ms": 8000,
  "entry": "preferred",
  "plugins": [
    {
      "tag": "cloudflare-preferred",
      "type": "cloudflare_preferred",
      "rewrite_ttl_secs": 60,
      "preferred": {},
      "optimizer": {
        "enabled": false
      }
    }
  ],
  "layers": [
    {
      "tag": "preferred",
      "type": "interceptor",
      "plugin": "cloudflare-preferred",
      "fallback": "cloudflare-doh"
    },
    {
      "tag": "cloudflare-doh",
      "type": "doh",
      "address": "1.1.1.1:443",
      "url": "https://cloudflare-dns.com/dns-query",
      "timeout_ms": 2800,
      "fallback": "tencent-doh"
    },
    {
      "tag": "tencent-doh",
      "type": "doh",
      "address": "120.53.53.53:443",
      "url": "https://doh.pub/dns-query",
      "timeout_ms": 2800,
      "fallback": "local"
    },
    {
      "tag": "local",
      "type": "udp",
      "address": "192.168.1.1:53",
      "timeout_ms": 1800
    }
  ]
}
```

`local` is not the system resolver. Put an explicit numeric address for a router, SmartDNS instance, or another resolver and do not overlap the listener.

## Top-level fields

| Field | Description |
| --- | --- |
| `listener.address` | UDP/TCP listen address. Default: `127.0.0.1:53`. |
| `listener.allow_remote` | Must be explicitly `true` for a non-loopback listener; default: `false`. |
| `cloudflare.range_refresh_secs` | Refresh period for official Cloudflare ranges; must be greater than zero. Failed refreshes keep the active list. |
| `request_timeout_ms` | Total deadline for a DNS request across the fallback chain. |
| `entry` | Layer tag used without a keyword match and for multi-question packets. |
| `plugins` | Statically built-in plugin definitions. Tags must be unique. |
| `layers` | Resolver/interceptor nodes. Tags must be unique, and declaration order decides keyword precedence. |

## Layer types

| `type` | Required fields | Behavior |
| --- | --- | --- |
| `udp` | `address` | DNS over UDP. A `TC=1` response is retried over TCP to the same address. |
| `tcp` | `address` | DNS over TCP. |
| `doh` | `address`, `url` | DNS over HTTPS. `address` is the fixed numeric bootstrap; the `url` hostname supplies SNI, Host, and certificate validation. |
| `dot` | `address`, `server_name` | DNS over TLS with mandatory certificate-name verification. |
| `interceptor` | `plugin`, `fallback` | Sends no network request; runs the built-in plugin after a downstream layer succeeds. |

Every network layer may set `fallback` and `timeout_ms`. Fallback references must exist and the graph cannot contain cycles. Network addresses cannot use port 0 or overlap the listener, including wildcard-address overlap.

### DoH constraints

The DoH `url` must use HTTPS and cannot contain credentials or a fragment. Its port must match the `address` port. EdgeSteer connects through the numeric bootstrap but keeps the URL hostname for TLS SNI, HTTP Host, and certificate validation; it disables inherited proxy settings, follows no redirects, and requires Content-Type `application/dns-message`.

### DoT constraints

DoT connects to `address` and uses `server_name` for TLS SNI and certificate verification. Self-signed certificates, wrong names, and handshake failures enter the layer's fallback.

## Keyword matching

A layer may declare `match`:

```json
{
  "mode": "label",
  "keywords": ["local", "lan"]
}
```

- `label` is the default. It matches a complete DNS label, case-insensitively; `printer.local` matches `local`, while `notlocal.example` does not. Label keywords cannot contain `.`.
- `contains` is explicit literal substring matching. It is not a regular-expression mode.
- Empty or whitespace-only keywords are rejected.
- For a single question, the first matching layer in declaration order wins; otherwise `entry` is used.
- Execution starts at the selected layer and follows only its fallback chain. A direct rule on Tencent or local skips an earlier preferred interceptor; put the match on the interceptor if the response must still be optimized.
- Multi-question packets always start at `entry` so one packet is not split across providers.

Example: route `.cn` names to Tencent and LAN labels to local:

```json
{
  "tag": "tencent-doh",
  "type": "doh",
  "address": "120.53.53.53:443",
  "url": "https://doh.pub/dns-query",
  "fallback": "local",
  "match": {
    "mode": "label",
    "keywords": ["cn"]
  }
}
```

## Preferred plugin and optimizer

The available plugin type is `cloudflare_preferred`. It is referenced only by an `interceptor` layer:

```json
{
  "tag": "cloudflare-preferred",
  "type": "cloudflare_preferred",
  "rewrite_ttl_secs": 60,
  "preferred": {
    "ipv4": "104.16.99.1",
    "ipv6": "2606:4700::1111"
  },
  "optimizer": {
    "enabled": true,
    "interval_secs": 21600,
    "test_host": "www.cloudflare.com",
    "test_path": "/cdn-cgi/trace",
    "test_port": 443,
    "timeout_ms": 3000,
    "concurrency": 16,
    "samples_per_cidr": 16,
    "max_candidates": 64,
    "candidates": ["104.16.0.0/13", "172.64.0.0/13"]
  }
}
```

Static `preferred.ipv4` and `preferred.ipv6` must be inside the active Cloudflare ranges. The optimizer samples IP or CIDR candidates; every candidate must pass TCP, TLS, and HTTP probes, with 2xx and `server: cloudflare` required. IPv4 and IPv6 are selected independently, and failures retain the last successful value.

The interceptor rewrites only when all relevant addresses are Cloudflare addresses. Mixed answers, non-Cloudflare addresses, missing preferred values, and responses without rewriteable records are returned unchanged. A rewrite sets TTL to `rewrite_ttl_secs` and clears DNSSEC authentication state.

## Validation checklist

`--check-config` validates JSON syntax, unknown fields, unique non-empty tags, entry/fallback/plugin references, fallback cycles, listener safety, network addresses and timeouts, DoH URL/port, DoT server name, keyword rules, and optimizer parameters. Static Cloudflare preferred addresses are also checked against the active ranges.
