# Operations

[English](operations.md) | [中文](../zh/operations.md) | [Back to English README](README.md)

This page covers building, validating, and using EdgeSteer as a local DNS service. Test the resolver graph on a high port first, then change system DNS; a configuration error should not take down DNS for the whole machine.

## Install and start

### Build from source

Rust 1.85 or newer is required:

```sh
git clone https://github.com/Fuxx-1/EdgeSteer.git
cd EdgeSteer
cp config.example.json "$HOME/edgesteer.json"
cargo build --locked --release
./target/release/edgesteer --check-config
```

PowerShell:

```powershell
Copy-Item config.example.json "$env:USERPROFILE\edgesteer.json"
cargo build --locked --release
.\target\release\edgesteer.exe --check-config
```

### Iced native UI

On macOS, open the matching `EdgeSteer-*-apple-darwin.dmg` release asset, drag `EdgeSteer.app` to Applications, and open it there. The disk image contains the UI and DNS engine together; port 53 may use a hidden elevated helper from that same bundle, but it does not install an `edgesteer` command-line daemon. If the Settings page detects a legacy root service, remove it there with administrator authorization.

`edgesteer-ui` is built with [Iced](https://github.com/iced-rs/iced), but it is a disposable settings window rather than the resolver host. The lightweight EdgeSteer Agent owns the menu bar, DNS engine, system-DNS state, and login integration; the window sends commands only over an authenticated loopback control channel. It edits the exact JSON used by the Agent, applies the same strict validation before saving, and atomically replaces the file so the watcher does not observe a partial document.

```sh
cargo build --locked --release --features gui
./target/release/edgesteer-ui
```

The service and UI both use the fixed `~/edgesteer.json` path (`%USERPROFILE%\edgesteer.json` on Windows), never another working-directory file. The UI covers the listener, resolver-layer `next` / `fallback` links, SRS rule sets, Cloudflare preferred-IP plugins, and the optimizer. It opens in Chinese dark mode; Settings provides Chinese/English and Dark/Light pick lists. The menu bar is the primary control surface, while Settings shows the Agent-managed listener, an optional per-user login item, and physical network-service state. On macOS, the packaged App runs as a menu-bar agent with no Dock entry. Closing Settings terminates the Iced process and releases its GPU/Metal resources; the menu-bar item opens a fresh Settings process when needed. Enabling system DNS requests administrator authorization only after the user selects it. Linux and Windows build and use the UI to configure the DNS service; their network managers remain responsible for system DNS registration. Linux menu-bar support requires GTK 3 and an Ayatana AppIndicator runtime.

### Test on a high port

Set the listener to `127.0.0.1:53535` and start it:

```sh
RUST_LOG=info ./target/release/edgesteer
```

From another terminal:

```sh
dig @127.0.0.1 -p 53535 www.cloudflare.com A +short
dig @127.0.0.1 -p 53535 example.cn A +short
```

On Windows:

```powershell
$env:RUST_LOG = "info"
.\target\release\edgesteer.exe
Resolve-DnsName www.cloudflare.com -Server 127.0.0.1 -Type A
```

`--check-config` validates JSON without binding a port. The service and App always read `~/edgesteer.json`; the macOS disk image does not use a root LaunchDaemon.

## Use as system DNS

Only consider switching to port 53 after high-port queries work. The default listener binds loopback and is not intended to be an open recursive DNS service.

On a DHCP macOS network, `type: "local"` can be used directly as system DNS: after a physical service points at the local listener, EdgeSteer reads that service's current DHCP option 6 DNS. It does not save old DNS addresses; a network change or DHCP renewal is picked up on the next refresh.

Do not write DHCP DNS addresses back as static settings when disabling it. In `EdgeSteer.app`, use Settings to enable system DNS only after the listener is ready. EdgeSteer records the affected service names, not historical DNS addresses. By default, closing Settings terminates the Iced process while the Agent keeps the resolver running. Choose `Quit EdgeSteer` explicitly from the menu bar to restore only those services to automatic DNS before the Agent stops the resolver and closes any Settings process. If that restoration fails, the App remains open instead of leaving system DNS on loopback. A no-snapshot workflow cannot faithfully restore user-entered manual DNS, so the App refuses to replace it.

`127.0.0.1:53535` is for testing or an explicit front end such as sing-box. Ordinary operating-system DNS settings have no port field, so direct takeover requires EdgeSteer on `127.0.0.1:53`. Linux and Windows still need their network managers to retain or expose the real underlay DNS.

Browser and application Secure DNS/DoH can bypass the system resolver. Disable per-application Secure DNS, or configure the application to use the local entry point, when EdgeSteer should process those queries.

## Hot reload

After a save, the watcher waits about 250 ms to coalesce editor events, then parses and validates the complete file. Valid JSON replaces the runtime snapshot. Invalid JSON, unknown fields, cycles, invalid upstreams, rule-set references, and static preferred addresses outside Cloudflare ranges are rejected while the last valid configuration remains active. When a rule-set definition changes, its worker reloads it immediately; a failed local or remote `.srs` reload keeps the prior successful version for that source.

Changes to `listener.address` or `allow_remote` cannot rebind sockets dynamically. The log reports that a restart is needed; the current listener remains active while other valid settings can still reload.

## Logs and troubleshooting

Use `RUST_LOG` to choose verbosity:

```sh
RUST_LOG=debug ./target/release/edgesteer
```

| Symptom | First checks |
| --- | --- |
| `configuration ... rejected` | JSON syntax, unknown fields, duplicate tags, fallback cycles, DoH URL/port, and listener/upstream overlap. |
| Query returns `SERVFAIL` | Check each layer's timeout, TLS, HTTP status, Content-Type, and endpoint; SERVFAIL is generated only after all network layers fail. |
| Cloudflare answer is not rewritten | In strict mode, the first request performs an SNI/Host check. Confirm that `compatibility_hosts` contains a real business hostname and the probe has not failed, then confirm the response addresses are in the active Cloudflare ranges. Returning the untouched answer before validation is the expected safe fallback. |
| Domain rule does not match | Label mode requires complete labels; only single-question packets use filters. A miss intentionally follows that layer's `next`; verify the `next` target as well as the keyword/rule set. For SRS, confirm a `loaded sing-box domain rule set` log entry, then check its tag, URL/path, and supported domain conditions. |
| Changes appear inactive | Wait for debounce and inspect reload logs. Listener changes require restart. Editors that save through a temporary empty file should be configured for atomic replacement. |
| DoH fails while direct IP works | Check URL hostname certificates, bootstrap port, system time, and network egress; EdgeSteer does not follow redirects and disables proxies. |

## Runtime safety

- `local` dynamically reads underlay DNS from system and DHCP network state; verify that startup logs show discovered addresses. A DHCP macOS network can still expose current physical-interface DNS after system DNS changes to the local listener. A manual or IPv6-only setup without DHCP option 6 must retain a visible real upstream and cannot be directly taken over by the no-snapshot system-DNS helper.
- EdgeSteer's native UDP/TCP sockets do not bypass sing-box TUN or transparent DNS interception. Proxy rules must route discovered underlay DNS `:53` traffic through the intended egress.
- Do not expose `allow_remote: true` directly to the public Internet; LAN deployments still need firewalling and rate limits.
- Preferred IP rewriting applies only after Cloudflare-range and SNI/Host validation. The default validation host, `blog.qoop.top`, validates only its own zone; add every other protected zone that needs preferred-IP rewriting to `compatibility_hosts`. Strict mode does not issue unverified candidates, although Cloudflare changing external routing after a DNS answer is issued is outside local-resolver control.
- The optimizer makes HTTPS probes to Cloudflare. Include that traffic in egress, proxy, and privacy reviews.
