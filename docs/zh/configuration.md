# 配置

[中文](configuration.md) | [English](../en/configuration.md) | [返回中文首页](../../README.md)

EdgeSteer 使用严格 JSON。每个带 `deny_unknown_fields` 的对象都会拒绝拼写错误或未实现的字段；修改配置前可以用 `--check-config` 只做校验而不启动 listener。

## 默认区域分流链路

`config.example.json` 的默认策略是“本地域名直连、国内优先腾讯、已知海外优先 CF、未知域名保留完整回退”。下面是可直接使用的完整结构：

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

声明顺序是策略的一部分，不能把 `cn-preferred` 或 `overseas-preferred` 放在 `local-keyword` 前面：这样 `b2c`、`mi`、`local` 才会先命中真实本地 DNS。其余分支都先经过 `cloudflare-preferred`，所以无论最终命中腾讯还是 CF 的 Cloudflare 地址，都会经过同一套 Cloudflare 网段校验和优选改写。

`local-keyword` 使用 `contains`，与 sing-box 的 `domain_keyword` 一样是字面子串匹配；因此 `mi` 会匹配任何含有这两个字符的域名。若只希望匹配完整 DNS label（例如 `work.be.mi.com` 中的 `mi`），将该层的 `mode` 改为 `label`。

- `geosite-cn` 来自 `sing-geosite` 的 `rule-set` 分支，国内集合走 Tencent DoH；网络或协议失败后再尝试 Cloudflare DoH，最后才走动态 local。
- `geosite-geolocation-!cn` 是已收录的海外域名集合；URL 中的 `!` 写为 `%21`。它直接走 Cloudflare DoH，失败后走动态 local。
- 未命中任一规则集的域名（以及多 question 请求）从 `entry: preferred` 开始，即：优选拦截 → Tencent DoH → Cloudflare DoH → 动态 local。
- `geosite-geolocation-!cn` 不是“所有非国内域名”的集合。规则集尚未收录或尚未加载的域名不会误走海外分支，而是使用上述默认链。

`local` 从操作系统的网络 DNS 配置读取真实上游，不调用系统 resolver，也不接受 `address`。macOS 枚举 SystemConfiguration 中非隧道网络服务（跳过 `utun`/`ppp`/`tun` 等虚拟接口）；当物理服务的配置 DNS 只有回环或 listener 地址时，改读同一服务 DHCP Option 6 中当前下发的 IPv4 DNS。Linux 读取 systemd-resolved 的真实 `resolv.conf`（存在时）或 `/etc/resolv.conf`，Windows 读取已启用网卡的 DNS 配置。EdgeSteer 对发现到的数值地址直接发送 DNS wire query；它会过滤回环、未指定、组播、IPv6 link-local、重复地址和自身 listener。

在 macOS 的 DHCP 网络上，即使系统 DNS 已被改成 `127.0.0.1`、`::1` 或 EdgeSteer 自身地址，`local` 仍会从物理服务的当前 DHCP 租约读取 DNS，不会回环，也不依赖旧地址快照。没有 DHCP Option 6 的手工或仅 IPv6 DNS 环境仍需让系统配置暴露一个可用的真实上游。原生 UDP/TCP socket 不会绕过 sing-box TUN 或透明 DNS 接管；这些路由规则需要在外层代理中处理。

## 顶层字段

| 字段 | 说明 |
| --- | --- |
| `listener.address` | UDP/TCP 监听地址。默认 `127.0.0.1:53`。 |
| `listener.allow_remote` | 非回环监听时必须显式为 `true`；默认 `false`。 |
| `cloudflare.range_refresh_secs` | 官方 Cloudflare 网段刷新周期，必须大于 0。刷新失败保留当前网段。 |
| `request_timeout_ms` | 单个 DNS 请求穿过整条 fallback 链的总 deadline。 |
| `entry` | 没有域名关键词或规则集命中，或请求包含多个 question 时使用的 layer tag。 |
| `plugins` | 静态 builtin 插件配置列表。tag 必须唯一。 |
| `rule_sets` | 可选的本地或远程 sing-box SRS 域名规则集。tag 必须唯一。 |
| `layers` | resolver/interceptor 节点列表。tag 必须唯一，声明顺序还决定域名匹配的先后。 |

## Layer 类型

| `type` | 必填字段 | 行为 |
| --- | --- | --- |
| `udp` | `address` | DNS over UDP。收到 `TC=1` 会对同一地址重试 TCP。 |
| `tcp` | `address` | DNS over TCP。 |
| `doh` | `address`, `url` | HTTPS DNS。`address` 是固定数值 bootstrap；`url` 主机名用于 SNI、Host 和证书校验。 |
| `dot` | `address`, `server_name` | DNS over TLS，必须校验证书名称。 |
| `local` | 无 | 动态读取系统网络 DNS，按顺序使用发现到的 UDP/TCP 上游。可设置 `timeout_ms` 和 `refresh_secs`。 |
| `interceptor` | `plugin`, `fallback` | 不发送网络请求，在后继 layer 成功后执行 builtin plugin。 |

所有网络 layer 都可以设置 `fallback` 和 `timeout_ms`。`local` 额外可设置 `refresh_secs`，默认 30 秒。fallback 引用必须存在，整张图不能有环。固定网络地址不能使用端口 0，也不能与 listener 地址重叠，包括未指定地址造成的潜在重叠。

### DoH 约束

DoH `url` 必须是 HTTPS，不能包含用户名、密码或 fragment；URL 的端口必须与 `address` 端口一致。程序用数值 bootstrap 连接，但保留 URL 主机名进行 TLS SNI、HTTP Host 和证书验证；不继承 HTTP 代理环境变量，不跟随重定向，并要求响应 Content-Type 为 `application/dns-message`。

### DoT 约束

DoT 使用 `address` 建立 TCP 连接，使用 `server_name` 完成 TLS SNI 和证书校验。自签证书、错误名称和握手失败都会进入该 layer 的 fallback。

### 动态 local

```json
{
  "tag": "local",
  "type": "local",
  "timeout_ms": 1800,
  "refresh_secs": 30
}
```

启动时立即发现系统 DNS，随后按 `refresh_secs` 刷新；同一进程中存在多个 `local` layer 时，使用最短刷新周期。单次 local 查询会依次尝试缓存中的每个地址，UDP 响应带 `TC=1` 时用同一地址 TCP 重试。一个地址出现网络或协议错误后，EdgeSteer 会在本次请求的剩余 deadline 内立即重新发现系统 DNS，并把新地址追加到候选列表。有效 DNS 响应（包括 `SERVFAIL`）不会触发重试或 fallback。

`local` 只能接受 `timeout_ms`、`refresh_secs`、`fallback` 和 `match`；`address`、`url`、`server_name`、`plugin` 会被拒绝。这里的“动态”是读取当前系统与 DHCP 网络状态，不是调用 libc resolver，因此不会自行进入 EdgeSteer listener，也不会把过去的 DNS 地址固定下来。

## sing-box SRS 域名规则集

`rule_sets` 原生读取 sing-box 二进制 `.srs`，无需安装或调用外部 `sing-box`。例如将 `geosite-private` 送至本地 DNS：

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

规则集有两种 source：

| `type` | 字段 | 默认刷新 | 说明 |
| --- | --- | --- | --- |
| `remote` | `url`，可选 `update_interval_secs`、`timeout_ms` | 24 小时 | 仅接受 HTTPS URL；URL 不允许凭据或 fragment。 |
| `local` | `path`，可选 `update_interval_secs` | 60 秒 | 从本地 `.srs` 文件重读；不接受 `url` 或 `timeout_ms`。 |

支持 sing-box SRS v1 到 v5 的 `domain`、`domain_suffix`、`domain_keyword`、`domain_regex` 及其 logical 组合。EdgeSteer 没有进程、端口、网卡或目的 IP 上下文，因此包含此类非域名条件的规则集会被拒绝，而不是被静默错误地匹配。

启动时立即加载，随后按 `update_interval_secs` 刷新。远程或本地更新解析失败时会保留上一份成功加载的规则；新规则集在首次加载成功前不会命中。规则集的加载结果原子替换，不会让同一次 DNS 查询读到半份新规则。

## 域名匹配

layer 可选 `match`：

```json
{
  "mode": "label",
  "keywords": ["local", "lan"],
  "rule_sets": ["geosite-private"]
}
```

- `label` 是默认模式。关键词按完整 DNS label 匹配，大小写不敏感；`printer.local` 会命中 `local`，`notlocal.example` 不会命中。label 关键词不能包含 `.`。
- `contains` 是显式字面子串匹配，适合确实需要宽松规则的场景；它不提供正则语义。
- `rule_sets` 引用顶层规则集 tag；同一 `match` 中关键词和规则集是“任一命中即可”的关系。规则集尚未加载、刷新失败且没有旧版本时不会命中。
- 关键词或规则集 tag 为空、未声明或重复都会被拒绝。
- 单 question 请求按 `layers` 的声明顺序取第一个关键词或规则集命中的 layer；没有命中才使用 `entry`。
- 命中后从目标 layer 开始，只走该节点自己的 fallback。因此把规则直接放在 Tencent 或 local 上会跳过之前的 preferred interceptor；若仍需优选，应把匹配规则放在 interceptor，或让目标路径经过 interceptor。
- 多 question 请求始终从 `entry` 开始，不把同一 DNS 报文拆给不同 resolver。

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
    "concurrency": 32,
    "samples_per_cidr": 40,
    "probes_per_candidate": 3,
    "compatibility_hosts": ["your-cf-domain.example"],
    "max_candidates": 640,
    "candidates": [
      "104.16.0.0/13", "104.24.0.0/14",
      "172.64.0.0/17", "172.64.128.0/18", "172.64.192.0/19",
      "172.64.224.0/22", "172.64.229.0/24", "172.64.230.0/23",
      "172.64.232.0/21", "172.64.240.0/21", "172.64.248.0/21",
      "172.67.0.0/16", "103.21.244.0/22", "103.31.4.0/22",
      "188.114.96.0/20", "172.66.0.0/16"
    ]
  }
}
```

候选不是 Cloudflare 官方或 [CloudflareSpeedTest 的 `ip.txt`](https://github.com/XIU2/CloudflareSpeedTest/blob/master/ip.txt) 的全量镜像。默认池包含官方的 `104.16.0.0/13` 与 `104.24.0.0/14`、`172.64.0.0/13` 的公共边缘部分、`172.67.0.0/16`，以及额外的 `103.21.244.0/22`、`103.31.4.0/22`、`188.114.96.0/20`、`172.66.0.0/16`。`172.64.228.0/24` 被明确排除，因为它有已知的 EIV 限制历史。`16` 个 CIDR 各采样 `40` 个地址，`max_candidates: 640` 恰好保证每个配置段均参与本轮。探测前 EdgeSteer 仍会按 Cloudflare 官方实时网段校验，因此过期或非 Cloudflare 地址不会成为优选 IP。静态 `preferred.ipv4`、`preferred.ipv6` 也必须位于当前 Cloudflare 网段。

`compatibility_hosts` 是针对实际业务域名的第二道筛选。将 `your-cf-domain.example` 替换为至少一个由 Cloudflare 代理的真实域名；每个候选在测速通过后，会用该域名的 SNI 和 Host 再请求一次。只有返回 2xx 或 3xx 的候选才会保留，因此 `403 error code: 1034`、WAF 拒绝和其他不兼容的边缘 IP 都会被淘汰。空数组表示不做此额外校验，不应在遇到 1034 的站点上使用。

optimizer 从 IP 或 CIDR 候选中采样；每个候选连续执行 `probes_per_candidate` 次完整 TCP、TLS 与 HTTP 探测，任一次失败即淘汰，HTTP 必须返回 2xx 且 `server: cloudflare`。它按“中位延迟 + 一半尾延迟”排序，避免单次偶发快或严重抖动的 IP 胜出。IPv4/IPv6 分开选择；某一地址族本轮没有合格候选时保留上一次成功值。

拦截器只在相关地址全部属于 Cloudflare 时改写。混合地址、非 Cloudflare 地址、没有优选值或没有可改写记录都会原样返回；实际改写后 TTL 设为 `rewrite_ttl_secs`，并清理 DNSSEC 认证状态。

## 校验清单

`--check-config` 会验证：JSON 语法、未知字段、非空并唯一的 tag、entry/fallback/plugin/规则集引用、fallback 环、listener 安全边界、网络地址和 timeout、DoH URL/端口、DoT server name、关键词、SRS 来源字段以及 optimizer 参数。Cloudflare 静态优选地址还会对当前活动网段做校验。
