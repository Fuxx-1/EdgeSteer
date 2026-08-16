# Configuration

[English](configuration.md) | [中文](../zh/configuration.md) | [Back to English README](README.md)

EdgeSteer uses strict JSON. Objects marked with `deny_unknown_fields` reject misspelled and unimplemented fields. Use `--check-config` before starting a listener.

## Default regional routing chain

`config.example.json` implements the default policy: local-name bypass, Tencent first for China, Cloudflare first for known overseas domains, and a complete fallback chain for unknown domains. This is the full usable structure:

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
  "rule_sets": [
    {
      "tag": "geosite-cn",
      "type": "remote",
      "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-cn.srs",
      "update_interval_secs": 86400,
      "timeout_ms": 10000
    },
    {
      "tag": "geosite-geolocation-not-cn",
      "type": "remote",
      "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-geolocation-%21cn.srs",
      "update_interval_secs": 86400,
      "timeout_ms": 10000
    }
  ],
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
      "tag": "local-keyword",
      "type": "local",
      "timeout_ms": 1800,
      "refresh_secs": 30,
      "match": {
        "mode": "contains",
        "keywords": ["b2c", "mi", "local"]
      }
    },
    {
      "tag": "cn-preferred",
      "type": "interceptor",
      "plugin": "cloudflare-preferred",
      "fallback": "tencent-doh",
      "match": {
        "rule_sets": ["geosite-cn"]
      }
    },
    {
      "tag": "overseas-preferred",
      "type": "interceptor",
      "plugin": "cloudflare-preferred",
      "fallback": "cloudflare-doh",
      "match": {
        "rule_sets": ["geosite-geolocation-not-cn"]
      }
    },
    {
      "tag": "preferred",
      "type": "interceptor",
      "plugin": "cloudflare-preferred",
      "fallback": "tencent-doh"
    },
    {
      "tag": "tencent-doh",
      "type": "doh",
      "address": "120.53.53.53:443",
      "url": "https://doh.pub/dns-query",
      "timeout_ms": 2800,
      "fallback": "cloudflare-doh"
    },
    {
      "tag": "cloudflare-doh",
      "type": "doh",
      "address": "1.1.1.1:443",
      "url": "https://cloudflare-dns.com/dns-query",
      "timeout_ms": 2800,
      "fallback": "local-fallback"
    },
    {
      "tag": "local-fallback",
      "type": "local",
      "timeout_ms": 1800,
      "refresh_secs": 30
    }
  ]
}
```

Declaration order is part of the policy. Keep `local-keyword` before `cn-preferred` and `overseas-preferred` so `b2c`, `mi`, and `local` reach the real local DNS first. Every other branch starts with `cloudflare-preferred`, so a Cloudflare answer from either Tencent or Cloudflare DoH goes through the same range check and preferred-address rewrite.

`local-keyword` uses `contains`, a literal substring match like sing-box `domain_keyword`; `mi` therefore matches any domain containing those two characters. Change that layer's `mode` to `label` if only a complete DNS label should match, such as the `mi` in `work.be.mi.com`.

- `geosite-cn`, from the `sing-geosite` `rule-set` branch, uses Tencent DoH first. A network or protocol failure then tries Cloudflare DoH, then dynamic local DNS.
- `geosite-geolocation-!cn` is the set of known overseas domains. Its `!` is URL-encoded as `%21`; it uses Cloudflare DoH first, then dynamic local DNS.
- A domain matching neither rule set (and every multi-question packet) starts at `entry: preferred`: preferred interceptor → Tencent DoH → Cloudflare DoH → dynamic local DNS.
- `geosite-geolocation-!cn` is not a set of every non-Chinese domain. A domain not included in, or not yet loaded from, that set does not get misclassified as overseas; it follows the default chain above.

`local` reads real upstreams from the operating system's network DNS configuration. It does not call the system resolver and does not accept `address`. On macOS it enumerates non-tunnel SystemConfiguration services, skipping virtual `utun`/`ppp`/`tun` interfaces; when a physical service has only loopback or listener DNS configured, it reads that service's current IPv4 DNS from DHCP option 6. Linux uses systemd-resolved's real `resolv.conf` when present, otherwise `/etc/resolv.conf`; Windows reads DNS settings from enabled adapters. EdgeSteer sends DNS wire queries directly to the discovered numeric addresses and filters loopback, unspecified, multicast, IPv6 link-local, duplicate, and listener addresses.

On a DHCP macOS network, `local` can still obtain the current physical-service DNS after system DNS changes to `127.0.0.1`, `::1`, or the EdgeSteer listener. It reads the live DHCP lease rather than looping or relying on an old-address snapshot. A manual or IPv6-only DNS setup without DHCP option 6 must still expose a usable real upstream. A native UDP/TCP socket does not bypass sing-box TUN or transparent DNS interception; configure those routes in the outer proxy.

## Top-level fields

| Field | Description |
| --- | --- |
| `listener.address` | UDP/TCP listen address. Default: `127.0.0.1:53`. |
| `listener.allow_remote` | Must be explicitly `true` for a non-loopback listener; default: `false`. |
| `cloudflare.range_refresh_secs` | Refresh period for official Cloudflare ranges; must be greater than zero. Failed refreshes keep the active list. |
| `request_timeout_ms` | Total deadline for a DNS request across the fallback chain. |
| `entry` | Layer tag used without a domain keyword or rule-set match and for multi-question packets. |
| `plugins` | Statically built-in plugin definitions. Tags must be unique. |
| `rule_sets` | Optional local or remote sing-box SRS domain rule sets. Tags must be unique. |
| `layers` | Resolver/interceptor nodes. Tags must be unique, and declaration order decides domain-match precedence. |

## Layer types

| `type` | Required fields | Behavior |
| --- | --- | --- |
| `udp` | `address` | DNS over UDP. A `TC=1` response is retried over TCP to the same address. |
| `tcp` | `address` | DNS over TCP. |
| `doh` | `address`, `url` | DNS over HTTPS. `address` is the fixed numeric bootstrap; the `url` hostname supplies SNI, Host, and certificate validation. |
| `dot` | `address`, `server_name` | DNS over TLS with mandatory certificate-name verification. |
| `local` | None | Dynamically reads system network DNS and tries its discovered UDP/TCP upstreams in order. `timeout_ms` and `refresh_secs` are optional. |
| `interceptor` | `plugin`, `fallback` | Sends no network request; runs the built-in plugin after a downstream layer succeeds. |

Every network layer may set `fallback` and `timeout_ms`. `local` can also set `refresh_secs`, which defaults to 30 seconds. Fallback references must exist and the graph cannot contain cycles. Fixed network addresses cannot use port 0 or overlap the listener, including wildcard-address overlap.

### DoH constraints

The DoH `url` must use HTTPS and cannot contain credentials or a fragment. Its port must match the `address` port. EdgeSteer connects through the numeric bootstrap but keeps the URL hostname for TLS SNI, HTTP Host, and certificate validation; it disables inherited proxy settings, follows no redirects, and requires Content-Type `application/dns-message`.

### DoT constraints

DoT connects to `address` and uses `server_name` for TLS SNI and certificate verification. Self-signed certificates, wrong names, and handshake failures enter the layer's fallback.

### Dynamic local

```json
{
  "tag": "local",
  "type": "local",
  "timeout_ms": 1800,
  "refresh_secs": 30
}
```

Discovery runs at startup and then every `refresh_secs`; when a process has more than one `local` layer, the shortest interval wins. A local query tries cached addresses in order and retries a `TC=1` UDP response over TCP to the same address. After an address has a network or protocol failure, EdgeSteer immediately rediscovers system DNS within the remaining request deadline and appends newly found addresses to that request's candidates. A valid DNS response, including `SERVFAIL`, does not retry or fall back.

`local` only accepts `timeout_ms`, `refresh_secs`, `fallback`, and `match`; `address`, `url`, `server_name`, and `plugin` are rejected. Dynamic means reading current system and DHCP network state rather than calling the libc resolver, so it does not itself enter the EdgeSteer listener or pin a historical DNS address.

## sing-box SRS domain rule sets

`rule_sets` reads binary sing-box `.srs` files natively; it neither installs nor invokes an external `sing-box` executable. For example, send `geosite-private` names to local DNS:

```json
{
  "rule_sets": [
    {
      "tag": "geosite-private",
      "type": "remote",
      "url": "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-private.srs",
      "update_interval_secs": 86400,
      "timeout_ms": 10000
    }
  ],
  "layers": [
    {
      "tag": "local-private",
      "type": "local",
      "timeout_ms": 1800,
      "refresh_secs": 30,
      "match": {
        "rule_sets": ["geosite-private"]
      }
    }
  ]
}
```

There are two rule-set source types:

| `type` | Fields | Default refresh | Behavior |
| --- | --- | --- | --- |
| `remote` | `url`, optional `update_interval_secs`, `timeout_ms` | 24 hours | HTTPS only; credentials and fragments are rejected. |
| `local` | `path`, optional `update_interval_secs` | 60 seconds | Re-reads a local `.srs`; `url` and `timeout_ms` are rejected. |

SRS v1 through v5 are supported for `domain`, `domain_suffix`, `domain_keyword`, `domain_regex`, and their logical combinations. EdgeSteer has no process, port, interface, or destination-IP context, so a rule set containing those non-domain conditions is rejected rather than silently matched incorrectly.

Rule sets load immediately at startup and refresh on `update_interval_secs`. A failed local or remote update retains the previous successful version; a new rule set does not match until its first successful load. A completed set is published atomically, so one DNS query never observes a partial update.

## Domain matching

A layer may declare `match`:

```json
{
  "mode": "label",
  "keywords": ["local", "lan"],
  "rule_sets": ["geosite-private"]
}
```

- `label` is the default. It matches a complete DNS label, case-insensitively; `printer.local` matches `local`, while `notlocal.example` does not. Label keywords cannot contain `.`.
- `contains` is explicit literal substring matching. It is not a regular-expression mode.
- `rule_sets` references top-level rule-set tags. Keywords and rule sets within one `match` are alternatives: either can select the layer. A not-yet-loaded set, or one without a previous successful version after a failed refresh, does not match.
- Empty, unknown, or duplicate keyword/rule-set references are rejected.
- For a single question, the first layer with a matching keyword or rule set in declaration order wins; otherwise `entry` is used.
- Execution starts at the selected layer and follows only its fallback chain. A direct rule on Tencent or local skips an earlier preferred interceptor; put the match on the interceptor if the response must still be optimized.
- Multi-question packets always start at `entry` so one packet is not split across providers.

## Preferred plugin and optimizer

The available plugin type is `cloudflare_preferred`. It is referenced only by an `interceptor` layer:

```json
{
  "tag": "cloudflare-preferred",
  "type": "cloudflare_preferred",
  "rewrite_ttl_secs": 60,
  "preferred": {},
  "optimizer": {
    "enabled": true,
    "interval_secs": 21600,
    "test_host": "www.cloudflare.com",
    "test_path": "/cdn-cgi/trace",
    "test_port": 443,
    "timeout_ms": 3000,
    "concurrency": 32,
    "samples_per_cidr": 40,
    "probes_per_candidate": 3,
    "compatibility_hosts": ["your-cf-domain.example"],
    "excluded_candidates": ["172.64.228.0/24"],
    "max_candidates": 1040,
    "candidates": [
      "173.245.48.0/20", "103.21.244.0/22", "103.22.200.0/22",
      "103.31.4.0/22", "141.101.64.0/18", "108.162.192.0/18",
      "190.93.240.0/20", "188.114.96.0/20", "197.234.240.0/22",
      "198.41.128.0/17", "162.158.0.0/15",
      "104.16.0.0/13", "104.24.0.0/14",
      "172.64.0.0/17", "172.64.128.0/18", "172.64.192.0/19",
      "172.64.224.0/22", "172.64.229.0/24", "172.64.230.0/23",
      "172.64.232.0/21", "172.64.240.0/21", "172.64.248.0/21",
      "172.65.0.0/16", "172.66.0.0/16", "172.67.0.0/16",
      "131.0.72.0/22"
    ]
  }
}
```

The candidate pool covers the 26 IPv4 CIDRs from [CloudflareSpeedTest's `ip.txt`](https://github.com/XIU2/CloudflareSpeedTest/blob/master/ip.txt) that safely intersect Cloudflare's published list. It does not copy `104.16.0.0/12`: its `104.28.0.0/14` part is absent from Cloudflare's current official ranges, so the configuration retains only precise `104.16.0.0/13` and `104.24.0.0/14` entries. `excluded_candidates` filters after sampling and before the official-range guard; the default explicitly excludes `172.64.228.0/24`, which has a history of EIV restrictions. Twenty-six CIDRs sampled at 40 addresses each with `max_candidates: 1040` put every configured range in each round. Official ownership proves only address identity, not compatibility with every Cloudflare zone.

When the optimizer is enabled, `compatibility_hosts` must contain at least one real Cloudflare-proxied business hostname. Each candidate first passes its speed probes, then performs `probes_per_candidate` consecutive SNI/Host probes for every compatibility host. It is selected only for a 2xx/3xx response with no `Error 1034`, `Edge IP Restricted`, or equivalent refusal marker. The default validation host is `blog.qoop.top`; it proves only its own zone, so add each other business hostname that needs preferred-IP rewriting.

This enables strict mode: startup ignores static `preferred`, and a failed or empty probe round clears the old value and returns the upstream Cloudflare DNS result. Each actual DNS hostname also receives a fresh SNI/Host check before its answer is rewritten; first use, failure, an in-flight check, or validation older than `rewrite_ttl_secs` returns the original answer. The cache cannot outlive the rewritten DNS TTL, so EdgeSteer does not actively issue an unverified preferred IP for the query hostname. Cloudflare can change external routing after a DNS answer has been issued, which is outside a local resolver's control.

The optimizer samples IP or CIDR candidates; it runs `probes_per_candidate` full TCP, TLS, and HTTP probes for every candidate, rejects it if any attempt fails, and requires a 2xx response with `server: cloudflare`. It ranks successful candidates by median latency plus half of their tail latency, preventing a single lucky or highly jittery result from winning. IPv4 and IPv6 are selected independently; in strict mode, a family without a qualified candidate does not retain its previous value.

The interceptor rewrites only when all relevant addresses are Cloudflare addresses. Mixed answers, non-Cloudflare addresses, missing preferred values, and responses without rewriteable records are returned unchanged. A rewrite sets TTL to `rewrite_ttl_secs` and clears DNSSEC authentication state.

## Validation checklist

`--check-config` validates JSON syntax, unknown fields, unique non-empty tags, entry/fallback/plugin/rule-set references, fallback cycles, listener safety, network addresses and timeouts, DoH URL/port, DoT server name, keyword rules, SRS source fields, and optimizer parameters. Static Cloudflare preferred addresses are also checked against the active ranges.
