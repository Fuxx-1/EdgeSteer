# 开发与发布

[中文](development.md) | [English](../en/development.md) | [返回中文首页](../../README.md)

EdgeSteer 是 Rust 2024 项目，MSRV 为 Rust 1.85。文档、配置和源码应保持同一套 JSON schema 与 fallback 语义；新增行为先补测试和配置示例，再更新中英文文档。

## 代码布局

| 路径 | 内容 |
| --- | --- |
| `src/config.rs` | JSON schema、反序列化和配置校验。 |
| `src/dns.rs` | UDP/TCP listener、DoH/DoT/UDP/TCP upstream 和响应校验。 |
| `src/plugins.rs` | builtin response interceptor。 |
| `src/optimizer.rs` | Cloudflare 优选探测。 |
| `src/ranges.rs` | Cloudflare 网段加载和刷新。 |
| `src/state.rs` | 运行时快照、DoH client 缓存与并发控制。 |
| `src/watcher.rs` | 配置热重载。 |
| `src/main.rs`, `src/lib.rs` | CLI、日志和进程生命周期。 |
| `config.example.json` | 可直接复制修改的配置样例。 |
| `.github/workflows/` | CI 与 tag release。 |

## 本地质量门禁

提交前运行：

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
```

只校验配置：

```sh
cargo run --release -- --config config.example.json --check-config
```

测试不依赖 CI 访问 Cloudflare 或 Tencent。网络集成测试使用本地 fake UDP endpoint，其他 transport 的关键约束通过配置校验和纯函数单元测试覆盖，重点验证响应关联校验、fallback 顺序、关键词、拦截器改写和热重载。

## 变更约定

- 配置对象继续使用严格 JSON；新增字段必须同步更新 `config.example.json`、配置文档和校验测试。
- 上游失败只能在网络/协议不可接受时触发 fallback；不要把有效 NXDOMAIN、NODATA、SERVFAIL 或 REFUSED 静默改写为另一家 resolver 的结果。
- plugin 仅接受静态 builtin 名称，不引入动态库、脚本或 shell 执行路径。
- 运行时状态通过单一 `ArcSwap<RuntimeConfig>` 快照读取，避免配置代际和 optimizer 代际错配。
- listener 重绑定需要明确的生命周期设计；当前实现把 listener 变化留给重启，不要在 watcher 中自行创建第二组 socket。

## CI

`.github/workflows/ci.yml` 在 Linux 上执行 rustfmt、Clippy 和测试，并在 Linux、macOS、Windows 上做 release build 与测试。CI 使用锁文件和 Rust 1.85 toolchain，开发者本地应尽量复现同样命令。

## Release

`.github/workflows/release.yml` 只响应 `v*` tag，并先校验语义版本：

- `v1.2.3` 创建普通 GitHub Release。
- `v1.2.3-alpha.1`、`v1.2.3-beta.1`、`v1.2.3-rc.1` 等带连字符的版本创建 pre-release。
- 不符合语义版本的 tag 会在构建前失败。

发布流程会为 Linux x86_64、macOS Intel、macOS Apple Silicon 和 Windows x86_64 构建归档，并在 release 中上传二进制、README、示例配置和 LICENSE。

示例：

```sh
git tag -a v0.3.0 -m "EdgeSteer v0.3.0"
git push origin v0.3.0
```

发布前至少确认工作区干净、`cargo test --locked --all-targets` 通过、配置示例可用，并且 release workflow 能在目标 runner 上完成构建。

## 文档维护

根目录 README 是中文默认入口；`docs/zh/` 和 `docs/en/` 的专题文件一一对应、保持同名。修改配置或运行行为时，先更新中文事实，再同步英文翻译，并检查相对链接目标存在。
