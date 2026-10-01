# Codex Speed

[English](README.md) | **简体中文**

监控 Codex 原生 OpenTelemetry metrics 的本地网页工具。前端嵌入 Rust 可执行文件，运行时不需要 Node.js、独立前端文件或远程服务器。

![白底浅色网页监控界面，使用合成预览数据](docs/assets/web-desktop.png)

*截图使用测试合成数据。*

## 构建与启动

构建需要 Rust 1.94+、Node.js 24+ 和 pnpm 12.8.1。以下命令均在项目根目录执行，pnpm workspace 统一管理前端依赖和开发/发布入口。

```sh
pnpm install --frozen-lockfile
pnpm build
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

页面始终选择一个模型；首次有数据时默认选择最近上报的模型，之后保留当前选择。暂无模型数据时，选择器显示等待状态。

主指标反映所选窗口内的统计，支持 15 分钟、1 小时、24 小时和 7 天。每个指标有自己的有效样本数。这些是**服务端报告的性能参考值**，不是客户端 bench。Service TBT 的内部口径涉及跨 engine calls 的聚合，所以倒数明确标为估算，不能等同于逐 token 实测吞吐。提供方返回的 IAPI、Engine 和额外耗时放在详情中。

输入已经包含缓存输入，不能相加。Token 指标按 turn/model 报告，计时观测有自己的范围，不强行拼成逐请求记录。缺失显示 `—`，原生报告的零值保留为零。至少 20 个观测后才显示 P95。无限尾桶或不同桶边界可能使分位数无法估算。

网页随批次实时更新；服务端计时通常要等计时事件返回，token 用量通常在 turn 结束后更新，并非生成过程中的逐 token 即时速度。趋势按时间分桶，最多 61 个桶，没有观测就留空。计时图展示桶内平均值；Token 用量图分别展示输入、缓存输入、输出的桶内合计，不堆叠或相加缓存输入。不使用整轮 TPS、工具耗时扣减或会话日志逆推。

## 数据与参数

SQLite 保存最近七天的数据，位置为系统的本地数据目录下 `codex-speed/metrics.sqlite`，Linux 通常是 `~/.local/share/codex-speed/`。重启后历史保留。建表定义在 `src/schema.sql`。启动时数据库加载失败会删除数据库及 WAL/SHM 文件，重建空库；不迁移或兼容旧结构，重建仍失败则退出。只保存模型名、计时/token 聚合、时间戳和流标识哈希，不保存提示词或原始遥测包。

| 参数 | 默认值 |
| --- | --- |
| `--host IP` | `0.0.0.0` |
| `--port PORT` | `4318` |
| `--data-dir PATH` | 系统本地数据目录 / `codex-speed` |

默认监听 `0.0.0.0`，其他电脑可通过 `http://<服务器 IP>:4318` 访问。若只需本机访问，使用 `--host 127.0.0.1`。远程 Codex 的导出地址也应填写服务器 IP；页面“连接 Codex”按当前访问地址生成命令。服务没有登录认证，访问范围由所在网络和防火墙控制。修改端口后，需要同步修改 Codex 的导出地址。支持 OTLP HTTP JSON、protobuf 和 gzip 请求。Delta 批次去重；Cumulative 首次建立基线，此后只计增量，基线持久化避免重启后重复统计。校验直方图标记、桶计数和时间范围，在采集或查询时清理过期数据。

## 开发

后端：Axum、Tokio、SQLx/SQLite、官方 `opentelemetry-proto` 类型、`rust-embed`。
前端：React 19.3、TypeScript 7、Vite 8、Tailwind 4、shadcn/ui（Base UI）、TanStack Query、Recharts 3。依赖由 `Cargo.lock` 和 `pnpm-lock.yaml` 锁定。

### Dev：前后端自动更新

首次安装开发工具与依赖：

```sh
cargo install watchexec-cli --locked
pnpm install --frozen-lockfile
```

一个命令同时启动前后端：

```sh
pnpm dev
```

打开 <http://127.0.0.1:5173>。前端使用 Vite + React Fast Refresh；Rust 修改由 Watchexec 自动重新编译并重启。concurrently 管理两个进程，Ctrl+C 一起停止；任一进程退出也会停止另一进程。Rust 编译错误会保留文件监听，修复后再次自动编译。后端重启期间 SSE 自动重连。

开发后端监听 `0.0.0.0:4318`，网页监听 `0.0.0.0:5173`，Vite 转发 `/api`、SSE 和 `/v1/metrics`。开发数据保存在 `target/dev-data/metrics.sqlite`，开发编译产物在 `target/dev-build/`。数据库与 release 分开，后端端口统一为 `4318`；dev 和 release 不能同时占用这个端口。

开发编译使用 `--no-default-features`，只启动 API 服务，不内嵌前端，也不依赖 `web/dist`。Codex 连接开发环境时使用 `http://127.0.0.1:4318/v1/metrics`；页面的连接命令直接指向当前服务器的后端端口 `4318`，与 release 一致。

### Release：单文件内嵌网页

```sh
pnpm build
./target/release/codex-speed
```

该命令先构建前端，再编译 release Rust 程序。默认启用 `embedded-web` feature，将 `web/dist` 嵌入二进制；运行时不依赖 Vite、Node.js 或磁盘前端文件。release 默认端口仍是 `4318`，数据库使用系统本地数据目录。默认 `cargo run` 同样启用内嵌页面，需要先构建前端；实时开发请用上面的 `dev` 命令。

```sh
pnpm lint
pnpm build:frontend
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --locked
pnpm browsers
pnpm test
```

Playwright 使用隔离的本地服务和数据库，验证真实 OTLP 接收链路、筛选、去重、桌面/笔记本布局和可访问性，并将预览截图写入 `docs/assets/`。
