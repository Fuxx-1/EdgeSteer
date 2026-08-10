# 运行与运维

[中文](operations.md) | [English](../en/operations.md) | [返回中文首页](../../README.md)

本页说明从构建、验证到作为本机 DNS 使用的步骤。先用高端口确认回退链，再切换系统 DNS；这样配置错误不会把整台机器的 DNS 一起中断。

## 安装与启动

### 从源码构建

需要 Rust 1.85 或更新版本：

```sh
git clone https://github.com/Fuxx-1/EdgeSteer.git
cd EdgeSteer
cp config.example.json edgesteer.json
cargo build --locked --release
./target/release/edgesteer --config edgesteer.json --check-config
```

Windows PowerShell：

```powershell
Copy-Item config.example.json edgesteer.json
cargo build --locked --release
.\target\release\edgesteer.exe --config edgesteer.json --check-config
```

### 先用高端口验证

把 listener 改为 `127.0.0.1:53535`，启动：

```sh
RUST_LOG=info ./target/release/edgesteer --config edgesteer.json
```

另开终端查询：

```sh
dig @127.0.0.1 -p 53535 www.cloudflare.com A +short
dig @127.0.0.1 -p 53535 example.cn A +short
```

Windows 可使用：

```powershell
$env:RUST_LOG = "info"
.\target\release\edgesteer.exe --config edgesteer.json
Resolve-DnsName www.cloudflare.com -Server 127.0.0.1 -Type A
```

`--check-config` 只校验 JSON，不会占用监听端口。运行时默认从 `edgesteer.json` 读取，也可以通过 `--config` 指定路径。

## 接入系统 DNS

只有高端口查询成功后才考虑切换到 53 端口。默认 listener 只绑定回环地址，不适合作为开放递归 DNS 服务。

配置含有 `type: "local"` 时，不要把操作系统 DNS 改成 EdgeSteer 自己：`local` 依赖当前系统网络配置来发现真实下层 DNS，替换后只会得到被过滤的 loopback/listener 地址。此模式适合让 sing-box 等 DNS 前端指向 EdgeSteer，同时保留系统网卡的 DHCP/VPN DNS；若必须把 EdgeSteer 直接设为系统 DNS，应使用明确固定的 UDP/TCP 上游而不是动态 `local`。

macOS：

```sh
# 仅在未使用 type: "local" 时设置 EdgeSteer 为系统 DNS
sudo networksetup -setdnsservers "Wi-Fi" 127.0.0.1
# 恢复 DHCP DNS
sudo networksetup -setdnsservers "Wi-Fi" Empty
```

Linux 和 Windows：仅在未使用动态 `local` 时，才在当前网络管理器或网卡 IPv4 DNS 设置中填入 `127.0.0.1`。如果 EdgeSteer 使用 53535 做测试，系统 DNS 仍应指向正式监听端口；客户端不能把端口写进普通 DNS 地址字段。

浏览器或应用的 Secure DNS/DoH 可能绕过系统 resolver。需要 EdgeSteer 处理这些查询时，应关闭该应用的独立 Secure DNS，或在应用中明确配置对应的本地入口。

## 配置热重载

保存配置文件后，watcher 会等待约 250 ms 合并编辑事件，再读取并完整校验。合法配置替换运行时快照；非法 JSON、未知字段、环、错误 upstream 或不在 Cloudflare 网段的静态优选值都会被拒绝，进程继续使用上一次合法配置。

修改 `listener.address` 或 `allow_remote` 不能动态重绑。日志会提示需要重启；当前 listener 保持不变，其他合法字段仍可以生效。

## 日志与排障

通过 `RUST_LOG` 调整日志级别：

```sh
RUST_LOG=debug ./target/release/edgesteer --config edgesteer.json
```

| 现象 | 优先检查 |
| --- | --- |
| `configuration ... rejected` | JSON 语法、未知字段、重复 tag、fallback 环、DoH URL/端口、listener 与 upstream 是否重叠。 |
| 查询返回 `SERVFAIL` | 查看每一层的超时、TLS、HTTP、Content-Type 和上游地址；所有网络层失败后才会生成 SERVFAIL。 |
| Cloudflare 地址没有改写 | 确认响应地址全部在活动 Cloudflare 网段，plugin 被 interceptor layer 引用，且 `preferred` 已有值或 optimizer 探测成功。 |
| 规则没有命中 | label 模式只匹配完整 label；单 question 才走规则，多 question 使用 `entry`；检查 layer 声明顺序。 |
| 修改配置后行为没变 | 等待 debounce，查看 reload 日志；若改了 listener，需要重启；若文件保存过程产生临时空文件，修复编辑器的原子保存方式。 |
| DoH 失败但可直连 IP | 检查 URL 主机名证书、bootstrap 端口、系统时间、网络出口；EdgeSteer 不跟随重定向且禁用代理。 |

## 运行安全

- `local` 从系统网络配置动态读取下层 DNS；启动后先检查日志是否发现了地址。系统 DNS 若已被改成 EdgeSteer 或仅剩 `127.0.0.1`/`::1`，会拒绝回环并导致 local 失败。
- EdgeSteer 的原生 UDP/TCP socket 不绕过 sing-box TUN 或透明 DNS 接管。将发现到的下层 DNS 的 `:53` 流量放行到正确出口由代理规则负责。
- 不要把 `allow_remote: true` 直接暴露到公网；局域网部署也应配合防火墙和速率限制。
- 优选 IP 只改变经过 Cloudflare 网段确认的响应，不保证第三方站点的源站、证书或应用层可用性。
- optimizer 会发起到 Cloudflare 的 HTTPS 探测，配置代理、出口网络或隐私策略时应将其纳入评估。
