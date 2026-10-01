# Codex Speed

[English](README.md) | **简体中文**

监控 Codex 原生 OpenTelemetry metrics 的本地网页工具。前端嵌入 Rust 可执行文件，运行时不需要 Node.js、独立前端文件或远程服务器。

![白底浅色网页监控界面，使用合成预览数据](docs/assets/web-desktop.png)

*截图使用测试合成数据。*

## 构建与启动

构建需要 Rust 1.94+、Node.js 24+ 和 pnpm 12.8.1。

```sh
cd web
pnpm install --frozen-lockfile
pnpm build
cd ..
cargo build --release --locked
./target/release/codex-speed
```

浏览器打开 <http://127.0.0.1:4318>。同一服务通过 `/v1/metrics` 接收遥测。

构建好前端后，也可以安装：

```sh
cargo install --path . --locked
codex-speed
```

## 连接 Codex

保持监控运行，在另一个终端启动 Codex：

```sh
OTEL_METRIC_EXPORT_INTERVAL=1000 codex --enable runtime_metrics \
  -c 'otel.metrics_exporter={otlp-http={endpoint="http://127.0.0.1:4318/v1/metrics",protocol="json"}}'
```

该命令请求运行时计时，并将原生 metrics 按约一秒的间隔导出。设置只对这次启动生效；已运行的 Codex 需要带这些设置重新启动。不修改 Codex 源码，不代理模型请求，也不需要导出日志或 traces。

若希望永久启用，将以下设置合并进用户级 `~/.codex/config.toml` 的现有表，不要重复添加同名表：

```toml
[features]
runtime_metrics = true

[otel]
metrics_exporter = { otlp-http = { endpoint = "http://127.0.0.1:4318/v1/metrics", protocol = "json" } }
```

导出间隔仍通过环境变量 `OTEL_METRIC_EXPORT_INTERVAL` 设置，单位毫秒。`runtime_metrics` 是实验性功能；计时数据是否返回取决于 Codex 版本、传输方式和提供方。原生采集曾在 Codex CLI 0.159.3 上验证。参见[官方配置文档](https://learn.chatgpt.com/docs/config-file/config-reference)。

## 指标口径

| 显示项 | 原生来源或计算方式 |
| --- | --- |
| 首 token 延迟 | `codex.responses_api_engine_service_ttft.duration_ms` 的样本平均值 |
| 估算 Decode 吞吐 | `1000 × TBT 样本数 ÷ TBT 总和`，来源为 `codex.responses_api_engine_service_tbt.duration_ms` |
| 输入、缓存输入、输出 tokens | `codex.turn.token_usage`，按 `token_type` 分别累加 |
| TTFT P50 / P95 | 从直方图估算；所有批次均只有一个观测时，使用精确值计算最近秩分位数 |

主指标反映所选窗口内的统计，支持 15 分钟、1 小时、24 小时和 7 天。每个指标有自己的有效样本数。这些是**服务端报告的性能参考值**，不是客户端 bench。Service TBT 的内部口径涉及跨 engine calls 的聚合，所以倒数明确标为估算，不能等同于逐 token 实测吞吐。提供方返回的 IAPI、Engine 和额外耗时放在详情中。

输入已经包含缓存输入，不能相加。Token 指标按 turn/model 报告，计时观测有自己的范围，不强行拼成逐请求记录。缺失显示 `—`，原生报告的零值保留为零。至少 20 个观测后才显示 P95。无限尾桶或不同桶边界可能使分位数无法估算。

网页随批次实时更新；服务端计时通常要等计时事件返回，token 用量通常在 turn 结束后更新，并非生成过程中的逐 token 即时速度。趋势按时间分桶，最多 61 个桶，没有观测就留空。不使用整轮 TPS、工具耗时扣减或会话日志逆推。

## 数据与参数

SQLite 保存最近七天的数据，位置为系统的本地数据目录下 `codex-speed/metrics.sqlite`，Linux 通常是 `~/.local/share/codex-speed/`。重启后历史保留。只保存模型名、计时/token 聚合、时间戳和流标识哈希，不保存提示词或原始遥测包。

| 参数 | 默认值 |
| --- | --- |
| `--port PORT` | `4318` |
| `--data-dir PATH` | 系统本地数据目录 / `codex-speed` |

只监听 IPv4 本机回环地址。修改端口后，需要同步修改 Codex 的导出地址。支持 OTLP HTTP JSON、protobuf 和 gzip 请求。Delta 批次去重；Cumulative 首次建立基线，此后只计增量，基线持久化避免重启后重复统计。校验直方图标记、桶计数和时间范围，在采集或查询时清理过期数据。

## 开发

后端：Axum、Tokio、SQLx/SQLite、官方 `opentelemetry-proto` 类型、`rust-embed`。
前端：React 19.3、TypeScript 7、Vite 8、Tailwind 4、shadcn/ui（Base UI）、TanStack Query、Recharts 3。依赖由 `Cargo.lock` 和 `web/pnpm-lock.yaml` 锁定。

热更新：先在 4318 端口启动 Rust 服务，再执行 `cd web && pnpm dev`。Vite 将 API 和 SSE 转发给后端。内嵌页面有改动时，需要重新构建 `web/dist` 和 Rust 程序。尚未构建前端时，Rust 构建会给出明确操作提示。

```sh
pnpm --dir web lint
pnpm --dir web build
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --locked
pnpm --dir web exec playwright install chromium
pnpm --dir web test
```

Playwright 使用隔离的本地服务和数据库，验证真实 OTLP 接收链路、筛选、去重、桌面/笔记本布局和可访问性，并将预览截图写入 `docs/assets/`。
