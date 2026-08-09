# 配置

[中文](configuration.md) | [English](../en/configuration.md) | [返回中文首页](../../README.md)

EdgeSteer 使用严格 JSON。每个带 `deny_unknown_fields` 的对象都会拒绝拼写错误或未实现的字段；修改配置前可以用 `--check-config` 只做校验而不启动 listener。

## 最小完整链路

下面的例子对应“优选拦截 -> Cloudflare DoH -> Tencent DoH -> local”的典型路径：

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

`local` 不是“系统 DNS”。请写入路由器、SmartDNS 或其他明确 resolver 的数值地址，并避免与 listener 相同或重叠。

## 顶层字段

| 字段 | 说明 |
| --- | --- |
| `listener.address` | UDP/TCP 监听地址。默认 `127.0.0.1:53`。 |
| `listener.allow_remote` | 非回环监听时必须显式为 `true`；默认 `false`。 |
| `cloudflare.range_refresh_secs` | 官方 Cloudflare 网段刷新周期，必须大于 0。刷新失败保留当前网段。 |
| `request_timeout_ms` | 单个 DNS 请求穿过整条 fallback 链的总 deadline。 |
| `entry` | 没有关键词命中，或请求包含多个 question 时使用的 layer tag。 |
| `plugins` | 静态 builtin 插件配置列表。tag 必须唯一。 |
| `layers` | resolver/interceptor 节点列表。tag 必须唯一，声明顺序还决定关键词规则的先后。 |

## Layer 类型

| `type` | 必填字段 | 行为 |
| --- | --- | --- |
| `udp` | `address` | DNS over UDP。收到 `TC=1` 会对同一地址重试 TCP。 |
| `tcp` | `address` | DNS over TCP。 |
| `doh` | `address`, `url` | HTTPS DNS。`address` 是固定数值 bootstrap；`url` 主机名用于 SNI、Host 和证书校验。 |
| `dot` | `address`, `server_name` | DNS over TLS，必须校验证书名称。 |
| `interceptor` | `plugin`, `fallback` | 不发送网络请求，在后继 layer 成功后执行 builtin plugin。 |

所有网络 layer 都可以设置 `fallback` 和 `timeout_ms`。fallback 引用必须存在，整张图不能有环。网络地址不能使用端口 0，也不能与 listener 地址重叠，包括未指定地址造成的潜在重叠。

### DoH 约束

DoH `url` 必须是 HTTPS，不能包含用户名、密码或 fragment；URL 的端口必须与 `address` 端口一致。程序用数值 bootstrap 连接，但保留 URL 主机名进行 TLS SNI、HTTP Host 和证书验证；不继承 HTTP 代理环境变量，不跟随重定向，并要求响应 Content-Type 为 `application/dns-message`。

### DoT 约束

DoT 使用 `address` 建立 TCP 连接，使用 `server_name` 完成 TLS SNI 和证书校验。自签证书、错误名称和握手失败都会进入该 layer 的 fallback。

## 关键词匹配

layer 可选 `match`：

```json
{
  "mode": "label",
  "keywords": ["local", "lan"]
}
```

- `label` 是默认模式。关键词按完整 DNS label 匹配，大小写不敏感；`printer.local` 会命中 `local`，`notlocal.example` 不会命中。label 关键词不能包含 `.`。
- `contains` 是显式字面子串匹配，适合确实需要宽松规则的场景；它不提供正则语义。
- 关键词为空或全是空白会被拒绝。
- 单 question 请求按 `layers` 的声明顺序取第一个命中的 layer；没有命中才使用 `entry`。
- 命中后从目标 layer 开始，只走该节点自己的 fallback。因此把规则直接放在 Tencent 或 local 上会跳过之前的 preferred interceptor；若仍需优选，应把匹配规则放在 interceptor，或让目标路径经过 interceptor。
- 多 question 请求始终从 `entry` 开始，不把同一 DNS 报文拆给不同 resolver。

示例：`.cn` 走 Tencent，局域网标签走 local：

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

## Preferred 插件与 optimizer

当前可用插件类型为 `cloudflare_preferred`。它只能由 `interceptor` layer 引用：

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

静态 `preferred.ipv4`、`preferred.ipv6` 必须位于当前 Cloudflare 网段。optimizer 只从 IP 或 CIDR 候选中采样；每个候选都要通过 TCP、TLS 与 HTTP 探测，HTTP 必须返回 2xx 且 `server: cloudflare`。IPv4/IPv6 分开选择最快地址，失败时保留上一次成功值。

拦截器只在相关地址全部属于 Cloudflare 时改写。混合地址、非 Cloudflare 地址、没有优选值或没有可改写记录都会原样返回；实际改写后 TTL 设为 `rewrite_ttl_secs`，并清理 DNSSEC 认证状态。

## 校验清单

`--check-config` 会验证：JSON 语法、未知字段、非空并唯一的 tag、entry/fallback/plugin 引用、fallback 环、listener 安全边界、网络地址和 timeout、DoH URL/端口、DoT server name、关键词以及 optimizer 参数。Cloudflare 静态优选地址还会对当前活动网段做校验。
