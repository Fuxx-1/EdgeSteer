# Operations

[English](operations.md) | [中文](../zh/operations.md) | [Back to English README](README.md)

This page covers building, validating, and using EdgeSteer as a local DNS service. Test the fallback chain on a high port first, then change system DNS; a configuration error should not take down DNS for the whole machine.

## Install and start

### Build from source

Rust 1.85 or newer is required:

```sh
git clone https://github.com/Fuxx-1/EdgeSteer.git
cd EdgeSteer
cp config.example.json edgesteer.json
cargo build --locked --release
./target/release/edgesteer --config edgesteer.json --check-config
```

PowerShell:

```powershell
Copy-Item config.example.json edgesteer.json
cargo build --locked --release
.\target\release\edgesteer.exe --config edgesteer.json --check-config
```

### Test on a high port

Set the listener to `127.0.0.1:53535` and start it:

```sh
RUST_LOG=info ./target/release/edgesteer --config edgesteer.json
```

From another terminal:

```sh
dig @127.0.0.1 -p 53535 www.cloudflare.com A +short
dig @127.0.0.1 -p 53535 example.cn A +short
```

On Windows:

```powershell
$env:RUST_LOG = "info"
.\target\release\edgesteer.exe --config edgesteer.json
Resolve-DnsName www.cloudflare.com -Server 127.0.0.1 -Type A
```

`--check-config` validates JSON without binding a port. The default file is `edgesteer.json`; use `--config` for another path.

## Use as system DNS

Only consider switching to port 53 after high-port queries work. The default listener binds loopback and is not intended to be an open recursive DNS service.

Do not change operating-system DNS to EdgeSteer when the configuration contains `type: "local"`: that layer discovers real underlay DNS from the current system network configuration, and replacing it leaves only filtered loopback/listener addresses. This mode fits a setup where sing-box or another DNS front end points to EdgeSteer while the network adapter retains DHCP/VPN DNS. If EdgeSteer must be the system DNS directly, use explicit fixed UDP/TCP upstreams instead of dynamic `local`.

macOS:

```sh
# Only when type: "local" is not used
sudo networksetup -setdnsservers "Wi-Fi" 127.0.0.1
# Restore DHCP DNS
sudo networksetup -setdnsservers "Wi-Fi" Empty
```

On Linux and Windows, set `127.0.0.1` as the active adapter or network manager's DNS server only when dynamic `local` is not used. During a 53535 test, the production listener must still use its configured port; ordinary DNS address fields do not include a port.

Browser and application Secure DNS/DoH can bypass the system resolver. Disable per-application Secure DNS, or configure the application to use the local entry point, when EdgeSteer should process those queries.

## Hot reload

After a save, the watcher waits about 250 ms to coalesce editor events, then parses and validates the complete file. Valid JSON replaces the runtime snapshot. Invalid JSON, unknown fields, cycles, invalid upstreams, and static preferred addresses outside Cloudflare ranges are rejected while the last valid configuration remains active.

Changes to `listener.address` or `allow_remote` cannot rebind sockets dynamically. The log reports that a restart is needed; the current listener remains active while other valid settings can still reload.

## Logs and troubleshooting

Use `RUST_LOG` to choose verbosity:

```sh
RUST_LOG=debug ./target/release/edgesteer --config edgesteer.json
```

| Symptom | First checks |
| --- | --- |
| `configuration ... rejected` | JSON syntax, unknown fields, duplicate tags, fallback cycles, DoH URL/port, and listener/upstream overlap. |
| Query returns `SERVFAIL` | Check each layer's timeout, TLS, HTTP status, Content-Type, and endpoint; SERVFAIL is generated only after all network layers fail. |
| Cloudflare answer is not rewritten | Confirm every relevant address is in the active Cloudflare ranges, the plugin is referenced by an interceptor, and `preferred` has a value or a successful optimizer result. |
| Keyword rule does not match | Label mode requires complete labels; only single-question packets use rules; inspect declaration order. |
| Changes appear inactive | Wait for debounce and inspect reload logs. Listener changes require restart. Editors that save through a temporary empty file should be configured for atomic replacement. |
| DoH fails while direct IP works | Check URL hostname certificates, bootstrap port, system time, and network egress; EdgeSteer does not follow redirects and disables proxies. |

## Runtime safety

- `local` dynamically reads underlay DNS from system network configuration; verify that startup logs show discovered addresses. If system DNS has already been changed to EdgeSteer or only `127.0.0.1`/`::1` remains, it refuses the loop and local queries fail.
- EdgeSteer's native UDP/TCP sockets do not bypass sing-box TUN or transparent DNS interception. Proxy rules must route discovered underlay DNS `:53` traffic through the intended egress.
- Do not expose `allow_remote: true` directly to the public Internet; LAN deployments still need firewalling and rate limits.
- Preferred IP rewriting only applies after Cloudflare-range verification. It does not guarantee the origin, certificate, or application behavior of a third-party site.
- The optimizer makes HTTPS probes to Cloudflare. Include that traffic in egress, proxy, and privacy reviews.
