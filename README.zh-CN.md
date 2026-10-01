# Codex Speed

[English](README.md) | **简体中文**

按模型汇总 Codex CLI 吞吐和首输出延迟的本地终端仪表盘。
读取现有日志，无需修改 provider 或代理请求。

## 快速开始

需要 Rust 1.88+。

```sh
cargo run --release
```

也可以安装后直接运行：

```sh
cargo install --path . --locked
codex-speed
```

日常推荐 release 构建，启动更快。首次扫描选中的日志，之后只读取新增内容。状态保存在内存中，重启时重建。

## 参数

| 参数 | 用途 | 默认值 |
| --- | --- | --- |
| `--codex-home PATH` | Codex 日志目录 | `$CODEX_HOME` 或 `~/.codex` |
| `--limit N` | 加载最近修改的 N 个受支持文件；`0` 取消文件数量限制 | `50` |
| `--hours N` | 统计最近 N 小时；`0` 包含全部已加载历史 | `24` |
| `--json` | 输出一次 JSON 快照 | 关闭 |
| `--demo` | 使用模拟数据预览 | 关闭 |

每个文件保留最新 **100 轮**。省略文件或轮次、读取或解析出错时显示 `PARTIAL`。`loaded history` 仅包含已保留的记录；切换时间窗口不会加载更多文件。

## 快捷键

| 按键 | 操作 |
| --- | --- |
| `↑/↓`、`j/k` | 选择模型 |
| `1 / 2 / 3 / 4` | 1 小时 / 24 小时 / 7 天 / 已加载历史 |
| `PgUp/PgDn`、`Home` | 滚动轮次 / 返回最新记录 |
| `c` | 显示或隐藏图表 |
| `r` | 刷新 |
| `q`、`Esc`、`Ctrl+C` | 退出 |

轮次列表最新在顶部。图表展示最近 30 个成功轮次，**左旧右新**；横轴为轮次顺序，不代表等间隔时间。灰线为所选窗口的 P50。终端至少 100 列、36 行时显示图表。界面时间使用本地时区，JSON 使用 UTC。

## 指标

| 指标 | 含义 |
| --- | --- |
| First P50 / P95 | 日志中首输出延迟的中位数 / 第 95 百分位，可能包含推理或工具输出 |
| Turn TPS P50 | 各轮「总输出 token ÷ 整轮耗时」的中位数 |
| Non-R P50 | 各轮「非推理输出 token ÷ 整轮耗时」的中位数 |
| Last TPS / First | 最新成功轮次的测量值 |

**TPS 包含工具执行和等待，不是流式生成速度。** 非推理输出也可能包含工具参数。任务复杂度和推理设置会影响模型间比较。

只有成功完成的轮次参与统计。缺失测量显示 `—`（JSON 中为 `null`），各指标独立计算有效样本数。P95 使用 nearest-rank 算法，小样本时参考价值有限。`Unfinished` 仅表示未记录结束事件，不代表进程仍在运行。

## 支持范围

读取 `sessions/` 和 `archived_sessions/` 中来源为 `cli`、`exec` 或 `vscode` 的未压缩 JSONL。监听文件变更，每两秒轮询兜底。时间指标是否可用取决于 Codex 日志版本。

暂不支持压缩归档、跨文件历史拼接、子代理汇总及服务端隐式模型路由。

## 开发

使用 Rust、Ratatui/Crossterm、Clap、Notify/Walkdir、Serde 和 Chrono，依赖通过 `Cargo.lock` 锁定。

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```
