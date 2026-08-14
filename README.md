# EdgeSteer

[![CI](https://github.com/Fuxx-1/EdgeSteer/actions/workflows/ci.yml/badge.svg)](https://github.com/Fuxx-1/EdgeSteer/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

中文 | [English](docs/en/README.md)

EdgeSteer 是一个跨 macOS、Linux 和 Windows 的本地 Rust DNS steering proxy。它通过 JSON 配置多级 upstream 回退，并用内置拦截器把已确认属于 Cloudflare 的地址改写为当前优选 IP。

EdgeSteer 判断的是 DNS 响应里的地址，而不是域名文字。一个站点即使域名和 CNAME 都不含 `cf` 或 `cloudflare`，只要返回的 A、AAAA、HTTPS 或 SVCB 地址属于 Cloudflare 官方网段，就可以进入优选流程；混合返回或无法确认时保持原响应。

## 工作方式

默认示例按关键词和 `sing-geosite` 规则集分流：

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

配置中的实际选择顺序是：

- `b2c`、`mi`、`local` 关键词 → 动态本地 DNS；
- `geosite-cn` → 优选拦截 → Tencent DoH → Cloudflare DoH → 动态本地 DNS；
- `geosite-geolocation-!cn` → 优选拦截 → Cloudflare DoH → 动态本地 DNS；
- 未被规则集收录的域名（及多 question 请求）→ 优选拦截 → Tencent DoH → Cloudflare DoH → 动态本地 DNS。

`geosite-geolocation-!cn` 不是“所有非中国域名”的补集，只覆盖该规则集已收录的海外域名；因此未知域名仍保留默认的完整回退链。三个带优选的分支都从 interceptor 开始：它先让后继 upstream 返回完整 DNS 响应，再在地址全部通过 Cloudflare 网段校验时改写 A、AAAA 以及 HTTPS/SVCB hints。改写会清除 DNSSEC 的 `AD`、`DO` 和 RRSIG，避免把已修改的数据标成已认证。

内置 optimizer 以 TCP + TLS + HTTP 探测 Cloudflare 地址，要求测试端点返回 2xx 且 `server: cloudflare`，并独立选择最快的 IPv4 与 IPv6。它是连通性和时延选择器，不等同于真实业务带宽测试。

## 快速开始

需要 Rust 1.85 或更新版本。发布包覆盖 Linux x86_64、macOS Intel、macOS Apple Silicon 和 Windows x86_64。

```sh
git clone https://github.com/Fuxx-1/EdgeSteer.git
cd EdgeSteer
cp config.example.json edgesteer.json
cargo build --locked --release
./target/release/edgesteer --config edgesteer.json --check-config
RUST_LOG=info ./target/release/edgesteer --config edgesteer.json
```

Windows PowerShell：

```powershell
Copy-Item config.example.json edgesteer.json
cargo build --locked --release
.\target\release\edgesteer.exe --config edgesteer.json --check-config
$env:RUST_LOG = "info"
.\target\release\edgesteer.exe --config edgesteer.json
```

首次运行建议使用高端口，不要立即改系统 DNS：

```json
{
  "listener": { "address": "127.0.0.1:53535", "allow_remote": false }
}
```

验证：

```sh
dig @127.0.0.1 -p 53535 www.cloudflare.com A +short
```

完整字段、DoH/DoT 约束、关键词、sing-box SRS 域名规则集和插件示例见[配置文档](docs/zh/configuration.md)。

## 文档导航

| 主题 | 中文 | English |
| --- | --- | --- |
| 项目介绍与快速开始 | 本页 | [README](docs/en/README.md) |
| 架构与请求生命周期 | [architecture.md](docs/zh/architecture.md) | [architecture.md](docs/en/architecture.md) |
| JSON 配置、匹配与插件 | [configuration.md](docs/zh/configuration.md) | [configuration.md](docs/en/configuration.md) |
| 安装、运行、热重载与排障 | [operations.md](docs/zh/operations.md) | [operations.md](docs/en/operations.md) |
| 开发、测试、CI 与发布 | [development.md](docs/zh/development.md) | [development.md](docs/en/development.md) |

## 重要边界

- 配置文件是严格 JSON，未知字段会被拒绝；`entry`、layer、fallback 和 plugin 引用必须存在，fallback 不能成环。
- `local` 动态读取真实网络 DNS，不调用系统 resolver，并过滤 loopback、listener 和虚拟隧道 DNS。macOS 物理服务若被改为本机 listener，会直接读取该服务 DHCP Option 6 中当前下发的 DNS；不保存或回写旧地址。
- 原生 UDP/TCP 查询不会绕过 sing-box TUN 或透明 DNS 接管；对下层 DNS 的直连路由由外层代理配置负责。
- 只有网络、TLS、HTTP、空响应、畸形 DNS 或响应关联校验失败才进入 fallback。有效的 NXDOMAIN、NODATA、SERVFAIL 和 REFUSED 会直接返回。
- plugin 只允许静态编译进程序的 builtin 实现，JSON 不会加载动态库、脚本或外部命令。
- 默认只监听回环地址。`allow_remote: true` 会把它变成局域网 DNS 服务，应由使用者自行承担访问控制和滥用风险。

## 许可证

GPL-3.0-only，见 [LICENSE](LICENSE)。EdgeSteer 与 Cloudflare 无隶属或背书关系。
