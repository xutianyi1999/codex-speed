# Codex Speed

**English** | [简体中文](README.zh-CN.md)

A local web dashboard for Codex's native OpenTelemetry metrics. The frontend is embedded in the Rust executable: no Node.js, frontend directory, or remote server is needed at runtime.

![Light web dashboard with synthetic preview metrics](docs/assets/web-desktop.png)

*Preview uses synthetic test data.*

## Build and start

Requires Rust 1.94+, Node.js 24+, and pnpm 12.8.1 for building.

```sh
cd web
pnpm install --frozen-lockfile
pnpm build
cd ..
cargo build --release --locked
./target/release/codex-speed
```

Open <http://127.0.0.1:4318>. The same server receives metrics at `/v1/metrics`.

To install after building the frontend:

```sh
cargo install --path . --locked
codex-speed
```

## Connect Codex

Keep the dashboard running. Start Codex in another terminal:

```sh
OTEL_METRIC_EXPORT_INTERVAL=1000 codex --enable runtime_metrics \
  -c 'otel.metrics_exporter={otlp-http={endpoint="http://127.0.0.1:4318/v1/metrics",protocol="json"}}'
```

This requests runtime timing metrics and exports native metrics about every second. It affects only that Codex process; an already-running Codex must be restarted with these settings. No Codex source changes or proxy are involved. Logs and traces need not be exported.

For persistent configuration, merge these settings into the existing user-level `~/.codex/config.toml` tables:

```toml
[features]
runtime_metrics = true

[otel]
metrics_exporter = { otlp-http = { endpoint = "http://127.0.0.1:4318/v1/metrics", protocol = "json" } }
```

The export interval still comes from `OTEL_METRIC_EXPORT_INTERVAL` (milliseconds). `runtime_metrics` is experimental; timing availability depends on the Codex version, transport and provider. This integration was probed with Codex CLI 0.159.3. See the [official configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).

## What is measured

| Display | Native source / calculation |
| --- | --- |
| First-token latency | Mean `codex.responses_api_engine_service_ttft.duration_ms` |
| Estimated decode throughput | `1000 × TBT sample count / TBT sum`, from `codex.responses_api_engine_service_tbt.duration_ms` |
| Input / cached input / output | Sums of `codex.turn.token_usage`, grouped by `token_type` |
| TTFT P50 / P95 | Histogram estimates; exact nearest-rank values when every exported point contains one observation |

The headline numbers describe the selected window (15 minutes, 1 hour, 24 hours, or 7 days). Each metric has its own sample count. These are **server-reported timing references**, not a client benchmark. Service TBT has an internal cross-engine-call aggregation scope; its inverse is explicitly an estimate, not measured per-token decode throughput. IAPI and engine/overhead timings are available in the details dialog when reported.

Input includes cached input: do not add them together. Token metrics are reported per turn/model, while timing observations have their own scope. Timing and token counts are not paired into request records. Missing fields remain `—`; native reported zero values remain zero. P95 is withheld until there are at least 20 observations. Infinite histogram tails or incompatible bucket layouts can leave quantiles unavailable.

Metrics arrive in periodic batches; server timings typically become available after a timing event, and token usage at turn completion. The dashboard does not show instantaneous per-token speed. Trends use up to 61 time buckets with gaps for missing measurements. No turn-duration, tool-time subtraction, or session-log inference is used.

## Storage and options

SQLite stores supported histogram points for seven days in the platform's local data directory under `codex-speed/metrics.sqlite` (on Linux, typically `~/.local/share/codex-speed/`). History survives restarts. Only model names, timing/token aggregates, timestamps and hashed stream identities are persisted, not prompts or raw telemetry envelopes.

| Option | Default |
| --- | --- |
| `--port PORT` | `4318` |
| `--data-dir PATH` | Platform local data directory / `codex-speed` |

The listener is bound to IPv4 loopback only. Update Codex's exporter endpoint if you change the port. HTTP OTLP JSON and protobuf, including gzip requests, are supported. Delta exports are deduplicated. Cumulative exports establish a baseline first, then record increments; the baseline is persisted to avoid counting old history after a restart. Histogram flags, bucket consistency and timing ranges are validated. Old records are removed during ingestion or snapshot refresh.

## Development

Backend: Axum, Tokio, SQLx/SQLite, official `opentelemetry-proto` types and `rust-embed`.
Frontend: React 19.3, TypeScript 7, Vite 8, Tailwind 4, shadcn/ui with Base UI, TanStack Query and Recharts 3. Dependencies are locked in `Cargo.lock` and `web/pnpm-lock.yaml`.

For hot reload, start the Rust server on port 4318, then `cd web && pnpm dev`. Vite proxies the API and SSE to the backend. Rebuild `web/dist` and the Rust executable for embedded frontend changes. The Rust build fails with instructions if the frontend has not been built.

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

Playwright starts an isolated local server and database, tests live OTLP ingestion, filtering, deduplication, desktop/laptop layouts and accessibility, and writes preview screenshots under `docs/assets/`.
