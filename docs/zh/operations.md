# 运行与运维

[中文](operations.md) | [English](../en/operations.md) | [返回中文首页](../../README.md)

本页说明从构建、验证到作为本机 DNS 使用的步骤。先用高端口确认 resolver 图，再切换系统 DNS；这样配置错误不会把整台机器的 DNS 一起中断。

## 安装与启动

### 从源码构建

需要 Rust 1.85 或更新版本：

```sh
git clone https://github.com/Fuxx-1/EdgeSteer.git
cd EdgeSteer
cp config.example.json "$HOME/edgesteer.json"
cargo build --locked --release
./target/release/edgesteer --check-config
```

Windows PowerShell：

```powershell
Copy-Item config.example.json "$env:USERPROFILE\edgesteer.json"
cargo build --locked --release
.\target\release\edgesteer.exe --check-config
```

### Iced 原生界面

macOS 请下载 Release 中对应架构的 `EdgeSteer-*-apple-darwin.dmg`，将 `EdgeSteer.app` 拖到“应用程序”后打开。磁盘镜像把界面与 DNS 引擎放在同一个 App bundle 内；监听 53 端口时会按需启动同 bundle 的隐藏授权 helper，不会安装 `edgesteer` 命令行守护进程。若检测到旧版 root 服务，可在设置页点击“移除旧版命令行服务”并授权清理。

`edgesteer-ui` 使用 [Iced](https://github.com/iced-rs/iced) 构建，但它只是可释放的配置窗口。轻量 EdgeSteer Agent 持有菜单栏、DNS 引擎、系统 DNS 与登录启动；窗口通过受限的本机控制通道向 Agent 发出命令。它编辑 Agent 运行时读取的同一份 JSON：保存前调用相同的严格校验，并以原子替换写入，避免 watcher 读取半份配置。

```sh
cargo build --locked --release --features gui
./target/release/edgesteer-ui
```

服务与界面都固定使用 `~/edgesteer.json`（Windows 使用 `%USERPROFILE%\edgesteer.json`），不会从工作目录读取另一份配置。界面覆盖 listener、layer 的 `next` / `fallback`、SRS 规则集、Cloudflare 优选插件和 optimizer。默认是中文暗色界面；语言和黑/白主题位于设置页的下拉框。菜单栏是启动、停止、系统 DNS、登录启动和退出的主入口，设置窗口显示 Agent 管理的 listener 与物理网络服务状态。macOS App 以菜单栏代理方式运行，不显示 Dock 图标；关闭窗口会结束 Iced 图形进程并释放 GPU/Metal 资源，可从菜单栏重新打开。macOS 上，只有点击启用系统 DNS 后才会请求管理员授权；Linux、Windows 可以构建和使用界面配置 DNS 服务，但系统 DNS 仍由对应网络管理器负责。Linux 菜单栏需要 GTK 3 与 Ayatana AppIndicator 运行时。

### 先用高端口验证

把 listener 改为 `127.0.0.1:53535`，启动：

```sh
RUST_LOG=info ./target/release/edgesteer
```

另开终端查询：

```sh
dig @127.0.0.1 -p 53535 www.cloudflare.com A +short
dig @127.0.0.1 -p 53535 example.cn A +short
```

Windows 可使用：

```powershell
$env:RUST_LOG = "info"
.\target\release\edgesteer.exe
Resolve-DnsName www.cloudflare.com -Server 127.0.0.1 -Type A
```

`--check-config` 只校验 JSON，不会占用监听端口。运行时和 App 固定读取 `~/edgesteer.json`；macOS DMG 不使用 root LaunchDaemon。

## 接入系统 DNS

只有高端口查询成功后才考虑切换到 53 端口。默认 listener 只绑定回环地址，不适合作为开放递归 DNS 服务。

macOS 的 DHCP 网络可以直接配合 `type: "local"` 使用：物理服务 DNS 被改为本机 listener 后，EdgeSteer 会读取该服务当前 DHCP Option 6 的 DNS。它不保存旧 DNS 地址；网络切换或 DHCP 续租后，下一个刷新周期会采用当前下发的地址。

解除时不要把 DHCP DNS 地址写回为静态值。使用 `EdgeSteer.app` 时，在设置页确认 listener 就绪后再启用系统 DNS。EdgeSteer 只记录自己接管的服务名，不保存历史 DNS 地址；默认关闭窗口会退出 Iced 配置进程，Agent 与 DNS 引擎继续运行。需要停止时，从菜单栏明确选择“退出 EdgeSteer”，Agent 会先把这些服务恢复为自动 DNS，再停止解析器并关闭设置窗口。恢复失败时 App 会保持运行，不会留下指向回环地址的失效 DNS。无快照模式无法无损恢复用户原先手工填写的 DNS，因此 App 会拒绝覆盖此类服务。

`127.0.0.1:53535` 只适合测试或由 sing-box 等前端显式指定；操作系统 DNS 地址没有端口字段，直接接管时 EdgeSteer 必须监听 `127.0.0.1:53`。Linux 和 Windows 仍需要由各自的网络管理器保留或暴露真实下层 DNS。

浏览器或应用的 Secure DNS/DoH 可能绕过系统 resolver。需要 EdgeSteer 处理这些查询时，应关闭该应用的独立 Secure DNS，或在应用中明确配置对应的本地入口。

## 配置热重载

保存配置文件后，watcher 会等待约 250 ms 合并编辑事件，再读取并完整校验。合法配置替换运行时快照；非法 JSON、未知字段、环、错误 upstream、规则集引用或不在 Cloudflare 网段的静态优选值都会被拒绝，进程继续使用上一次合法配置。规则集定义变更后，worker 会立即重新加载；远程或本地 `.srs` 加载失败时继续保留同一来源的上一份成功版本。

修改 `listener.address` 或 `allow_remote` 不能动态重绑。日志会提示需要重启；当前 listener 保持不变，其他合法字段仍可以生效。

## 日志与排障

通过 `RUST_LOG` 调整日志级别：

```sh
RUST_LOG=debug ./target/release/edgesteer
```

| 现象 | 优先检查 |
| --- | --- |
| `configuration ... rejected` | JSON 语法、未知字段、重复 tag、`next + fallback` 图环、DoH URL/端口、listener 与 upstream 是否重叠。 |
| 查询返回 `SERVFAIL` | 查看每一层的超时、TLS、HTTP、Content-Type 和上游地址；所有网络层失败后才会生成 SERVFAIL。 |
| Cloudflare 地址没有改写 | 严格模式下首个请求会先做 SNI/Host 验证；确认 `compatibility_hosts` 包含真实业务域名、探测未失败，且响应地址全部在活动 Cloudflare 网段。未验证时原样返回是预期安全回退。 |
| 域名规则没有命中 | label 模式只匹配完整 label；单 question 才使用过滤，多 question 会跳过过滤层。未命中会按设计进入该 layer 的 `next`，同时检查 `next` 目标和关键词/规则集。SRS 规则集还要确认日志出现 `loaded sing-box domain rule set`，并检查其 tag、URL/路径和支持的域名条件。 |
| 修改配置后行为没变 | 等待 debounce，查看 reload 日志；若改了 listener，需要重启；若文件保存过程产生临时空文件，修复编辑器的原子保存方式。 |
| DoH 失败但可直连 IP | 检查 URL 主机名证书、bootstrap 端口、系统时间、网络出口；EdgeSteer 不跟随重定向且禁用代理。 |

## 运行安全

- `local` 从系统和 DHCP 网络状态动态读取下层 DNS；启动后先检查日志是否发现了地址。macOS DHCP 网络在系统 DNS 指向本机 listener 后仍可读取当前的物理网卡 DNS；没有 DHCP Option 6 的手工或仅 IPv6 环境需保留可见的真实上游，不能以无快照方式直接接管系统 DNS。
- EdgeSteer 的原生 UDP/TCP socket 不绕过 sing-box TUN 或透明 DNS 接管。将发现到的下层 DNS 的 `:53` 流量放行到正确出口由代理规则负责。
- 不要把 `allow_remote: true` 直接暴露到公网；局域网部署也应配合防火墙和速率限制。
- 优选 IP 只改变经过 Cloudflare 网段及 SNI/Host 验证的响应。默认验证主机 `blog.qoop.top` 只验证自身；任何其他需要优选改写的业务 zone 都应写入 `compatibility_hosts`。严格模式不会下发未验证候选，但 Cloudflare 在已发出 DNS 答案后改变其外部路由不受本地程序控制。
- optimizer 会发起到 Cloudflare 的 HTTPS 探测，配置代理、出口网络或隐私策略时应将其纳入评估。
