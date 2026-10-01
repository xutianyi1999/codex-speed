# Codex Speed

用 Rust 编写的 Codex CLI 本地终端仪表盘。读取现有会话日志，不修改 provider，
不代理请求，不读取认证文件，也不保存提示词或模型回复。

## 运行

```sh
cargo run -- --demo                 # 演示界面，不读取真实日志
cargo run                          # 自动读取 CODEX_HOME 或 ~/.codex
cargo run -- --codex-home /path/to/.codex
cargo run -- --json                 # 输出一次 JSON 快照
cargo run -- --demo --json          # 演示 JSON 快照
cargo run -- --hours 0              # 汇总已加载的全部历史
```

独立安装：

```sh
cargo install --path . --locked
codex-speed
```

按 `↑/↓` 或 `j/k` 选择模型，下方查看该模型的最近轮次。`r` 刷新，
`q`、`Esc` 或 `Ctrl+C` 退出。`1/2/3/4` 切换最近 1 小时、24 小时、7 天、全部已加载历史。
`PgUp/PgDn` 滚动轮次，`Home` 回到最新记录；切换模型或窗口后回到列表顶部。
模型表按数量自动调整高度，宽终端增加 Open、Fail、Last TPS、Last First 列。
模型详情显示最新成功轮次的指标和时间、各项有效样本数、未完成/失败/中断数量。
首输出延迟样本少于 20 时提示小样本，P95 应谨慎解读；Open 仅表示缺少结束事件。
轮次时间按运行机器的本地时区显示（遵循系统时区或 `TZ` 环境变量）；JSON 时间保留 UTC。

## 指标口径

| 显示项 | 来源与含义 |
| --- | --- |
| First P50 / P95 | 首输出延迟的中位数 / 第 95 百分位。来自 `time_to_first_token_ms`，包括首次推理或工具输出，不等于首段可见文字时间。 |
| TPS P50 | 各轮 `output_tokens / duration_ms` 的中位数，包含推理、工具执行和等待。 |
| Non-R P50 | 各轮 `(output_tokens - reasoning_output_tokens) / duration_ms` 的中位数；分母仍是整轮耗时，输出也可能包含工具参数。 |
| Done | 时间窗口内完成的轮次数；每项指标的有效样本数显示在下方面板标题中。 |
| Duration | Codex 保存的整轮毫秒耗时，不使用文件修改时间估算。 |
| Output | 模型报告的整轮 output tokens，包含 reasoning tokens。 |

按每个轮次记录的模型跨会话汇总，模型切换前后的轮次分别归组。默认最近 24 小时，
`--hours 0` 汇总已加载的全部历史；按结束时间筛选，缺失时回退到开始时间。
只将成功完成的轮次纳入指标，中断和失败单独计数。每项指标独立过滤缺失值，
P50 为中位数，P95 使用 nearest-rank 算法；小样本的 P95 接近最大值。

界面随日志更新，这些速度在轮次结束后计算，**不是实时流式生成速度或模型解码吞吐**。
不同任务、推理强度和工具耗时会影响模型间比较。
缺失数据显示 `—`，JSON 中为 `null`；不会用字符数、缺失的 reasoning 用量或整数秒
时间戳假造测速。未收到完成事件的轮次显示 `open`；这不保证对应进程仍然存活。

## 当前支持范围

- 从 `sessions/` 和 `archived_sessions/` 增量读取未压缩 `.jsonl`。
- 加载 `session_meta.source` 为 `cli`、`exec` 或 `vscode` 的会话。部分入口会将交互日志标记为 `vscode`，这些日志使用相同的指标格式；子代理不纳入统计。
- 默认加载最近修改的 50 个受支持的会话文件，`--limit 1..1000` 可调整。
- 每个会话保留最近 100 轮，重启后由日志重建，无需数据库。
- 优先使用带 turn/response ID 的 `token_usage_record.turn_token_usage`，按 response ID 去重。
- 旧日志回退到 `token_count.info.total_token_usage` 相对轮次开始时累计值的增量；
  累计值回退时丢弃该次估计，直到能再次计算非负增量。
- 保留未写完的 JSONL 尾行，跳过并统计完整但损坏的行。
- Notify 监听变更；每两秒发现新文件并检查追加内容，通知不可用时继续轮询。
- Linux/macOS 可识别 inode 更换，所有平台支持文件截断后的重新读取。

日志格式是 Codex 内部格式，不同版本可能缺少 duration、TTFT 或关联 ID。
压缩归档、跨文件历史分片拼接、子代理汇总暂不支持。模型名称来自各轮的
`turn_context`，无法确认时归为 `unknown`；不识别服务端隐式模型路由。性能依赖会话目录规模，首次读取较大的
文件可能花费一些时间。

## 实现

采用 Rust 2024 edition，直接依赖从 crates.io 选择当前最新稳定版，并通过 Cargo.lock
锁定构建。Ratatui/Crossterm 负责终端界面，Clap 负责 CLI，Notify/Walkdir 负责文件发现
和监听，Serde/Serde JSON 解析数据，Chrono 处理时间，Dirs 查找用户目录。

- `src/session.rs`：日志解析、轮次关联和指标计算。
- `src/monitor.rs`：文件发现、增量读取和 JSON 快照。
- `src/models.rs`：按模型聚合、时间窗口和分位数统计。
- `src/ui.rs`：终端面板。
- `src/main.rs`：参数、监听和交互循环。

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

后续可以添加每次请求的流式计时采集；只有取得请求开始、第一/最后文本事件以及最终
token 用量后，才能提供独立的可见文本 TTFT 和流式 TPS。
