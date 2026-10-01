use crate::metrics::{Histogram, Parsed};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicI64, Ordering},
};

const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub struct Store {
    pool: SqlitePool,
    last_pruned: AtomicI64,
}

#[derive(Default, Serialize)]
pub struct Distribution {
    pub samples: u64,
    pub mean_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub quantiles_approximate: bool,
}

#[derive(Default)]
struct Aggregate {
    count: u64,
    sum: f64,
    histograms: Vec<Histogram>,
}

impl Aggregate {
    fn add(&mut self, histogram: Histogram) {
        self.count += histogram.count;
        self.sum += histogram.sum;
        self.histograms.push(histogram);
    }

    fn mean(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum / self.count as f64)
    }

    fn quantile(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        let rank = (self.count as f64 * q).ceil() as u64;
        if self.histograms.iter().all(|h| h.count == 1) {
            let mut values: Vec<_> = self.histograms.iter().map(|h| h.sum).collect();
            values.sort_by(f64::total_cmp);
            return values.get(rank.saturating_sub(1) as usize).copied();
        }
        let first = self.histograms.first()?;
        if first.buckets.is_empty()
            || self
                .histograms
                .iter()
                .any(|h| h.bounds != first.bounds || h.buckets.len() != first.buckets.len())
        {
            return None;
        }
        let mut counts = vec![0_u64; first.buckets.len()];
        for h in &self.histograms {
            for (total, value) in counts.iter_mut().zip(&h.buckets) {
                *total += value;
            }
        }
        let mut below = 0;
        for (i, count) in counts.iter().enumerate() {
            if below + count >= rank {
                let upper = *first.bounds.get(i)?; // Don't invent an upper bound for +infinity.
                let lower = i.checked_sub(1).map_or(0.0, |j| first.bounds[j]);
                return Some(lower + (upper - lower) * (rank - below) as f64 / *count as f64);
            }
            below += count;
        }
        None
    }

    fn distribution(&self) -> Distribution {
        Distribution {
            samples: self.count,
            mean_ms: self.mean(),
            p50_ms: self.quantile(0.5),
            p95_ms: (self.count >= 20).then(|| self.quantile(0.95)).flatten(),
            quantiles_approximate: self.histograms.iter().any(|h| h.count > 1),
        }
    }
}

#[derive(Default, Serialize)]
pub struct Attempts {
    pub total: u64,
    pub failed: u64,
    pub failure_percent: f64,
}

#[derive(Default, Serialize)]
pub struct Summary {
    pub ttft: Distribution,
    pub tbt: Distribution,
    pub decode_tps: Option<f64>,
    pub input_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub cached_input_percent: Option<f64>,
    pub http_attempts: Option<Attempts>,
    pub websocket_send_attempts: Option<Attempts>,
    pub token_samples: BTreeMap<String, u64>,
    pub last_observation_ms: Option<i64>,
    pub details: BTreeMap<String, Distribution>,
}

#[derive(Default)]
struct SummaryBuilder {
    metrics: BTreeMap<String, Aggregate>,
    last: Option<i64>,
}

impl SummaryBuilder {
    fn add(&mut self, kind: &str, h: Histogram, time: i64) {
        self.last = Some(self.last.map_or(time, |old| old.max(time)));
        self.metrics.entry(kind.to_owned()).or_default().add(h);
    }

    fn finish(&self) -> Summary {
        let distribution = |key: &str| {
            self.metrics
                .get(key)
                .map_or_else(Distribution::default, Aggregate::distribution)
        };
        let tokens = |key: &str| self.metrics.get(key).map(|m| m.sum);
        let attempts = |success: &str, failed: &str| {
            let count = |key: &str| self.metrics.get(key).map_or(0, |m| m.count);
            let failed = count(failed);
            let total = count(success) + failed;
            (total > 0).then(|| Attempts {
                total,
                failed,
                failure_percent: 100.0 * failed as f64 / total as f64,
            })
        };
        let cached_input_percent = self
            .metrics
            .get("input")
            .zip(self.metrics.get("cached_input"))
            .filter(|(input, cached)| {
                input.count == cached.count && input.sum > 0.0 && cached.sum <= input.sum
            })
            .map(|(input, cached)| 100.0 * cached.sum / input.sum);
        Summary {
            ttft: distribution("ttft"),
            tbt: distribution("tbt"),
            decode_tps: self
                .metrics
                .get("tbt")
                .filter(|m| m.count > 0 && m.sum > 0.0)
                .map(|m| 1000.0 * m.count as f64 / m.sum),
            input_tokens: tokens("input"),
            cached_input_tokens: tokens("cached_input"),
            output_tokens: tokens("output"),
            reasoning_output_tokens: tokens("reasoning_output"),
            cached_input_percent,
            http_attempts: attempts("http_success", "http_failed"),
            websocket_send_attempts: attempts("websocket_send_success", "websocket_send_failed"),
            token_samples: ["input", "cached_input", "output", "reasoning_output"]
                .into_iter()
                .filter_map(|k| self.metrics.get(k).map(|m| (k.to_owned(), m.count)))
                .collect(),
            last_observation_ms: self.last,
            details: ["engine", "overhead", "iapi_ttft", "iapi_tbt"]
                .into_iter()
                .filter_map(|k| {
                    self.metrics
                        .get(k)
                        .map(|m| (k.to_owned(), m.distribution()))
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct ModelSummary {
    pub model: String,
    #[serde(flatten)]
    pub summary: Summary,
}

#[derive(Serialize)]
pub struct TrendPoint {
    pub time_ms: i64,
    pub ttft_ms: Option<f64>,
    pub decode_tps: Option<f64>,
    pub input_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub reasoning_output_tokens: Option<f64>,
    pub http_failure_percent: Option<f64>,
    pub websocket_send_failure_percent: Option<f64>,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub now_ms: i64,
    pub last_received_ms: Option<i64>,
    pub window_minutes: u32,
    pub endpoint: String,
    pub selected_model: Option<String>,
    pub models: Vec<ModelSummary>,
    pub summary: Summary,
    pub trend: Vec<TrendPoint>,
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        match Self::open_once(path).await {
            Ok(store) => Ok(store),
            Err(error) => {
                eprintln!(
                    "Cannot load metrics database ({error}); deleting it and creating an empty database"
                );
                for suffix in ["", "-wal", "-shm"] {
                    let mut file = path.as_os_str().to_os_string();
                    file.push(suffix);
                    match std::fs::remove_file(Path::new(&file)) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => {
                            return Err(error).context("Cannot delete failed metrics database");
                        }
                    }
                }
                Self::open_once(path)
                    .await
                    .context("Cannot create a fresh metrics database")
            }
        }
    }

    async fn open_once(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        let result = async {
            let check: String = sqlx::query_scalar("PRAGMA quick_check")
                .fetch_one(&pool)
                .await?;
            if check != "ok" {
                bail!("SQLite integrity check failed: {check}");
            }
            sqlx::raw_sql(include_str!("schema.sql"))
                .execute(&pool)
                .await?;
            // Validate every table used by the current implementation; never migrate old schemas.
            sqlx::raw_sql(
                "SELECT stream, start, end, time_ms, model, kind, histogram FROM samples LIMIT 0;
                SELECT stream, start, end, histogram, updated_ms FROM cumulative LIMIT 0;
                SELECT id, last_received_ms FROM receiver LIMIT 0;",
            )
            .execute(&pool)
            .await?;
            let store = Self {
                pool: pool.clone(),
                last_pruned: AtomicI64::new(0),
            };
            store.prune(chrono::Utc::now().timestamp_millis()).await?;
            Ok(store)
        }
        .await;
        if result.is_err() {
            pool.close().await;
        }
        result
    }

    async fn prune(&self, now: i64) -> Result<()> {
        let previous = self.last_pruned.load(Ordering::Relaxed);
        if now - previous < 60_000
            || self
                .last_pruned
                .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return Ok(());
        }
        let cutoff = now - RETENTION_MS;
        sqlx::query("DELETE FROM samples WHERE time_ms < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM cumulative WHERE updated_ms < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn ingest(&self, parsed: Parsed, now: i64) -> Result<(usize, i64)> {
        self.prune(now).await?;
        // Acquire the write lock before reading baselines to serialize simultaneous exporters.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut accepted = 0;
        let mut rejected = parsed.rejected;
        for point in parsed.points {
            let time = (point.end / 1_000_000) as i64;
            if time > now + 60_000 {
                rejected += 1;
                continue;
            }
            if time < now - RETENTION_MS {
                continue;
            }
            let start = point.start.to_string();
            let end = point.end.to_string();
            let mut h = point.histogram.clone();
            if point.cumulative {
                let previous = sqlx::query(
                    "SELECT end, histogram FROM cumulative WHERE stream = ? AND start = ?",
                )
                .bind(&point.stream)
                .bind(&start)
                .fetch_optional(&mut *tx)
                .await?;
                let baseline_only = previous.is_none();
                if let Some(previous) = previous {
                    let previous_end: String = previous.try_get("end")?;
                    if point.end <= previous_end.parse::<u64>()? {
                        continue;
                    }
                    let previous: Histogram =
                        serde_json::from_str(previous.try_get::<&str, _>("histogram")?)?;
                    let Some(delta) = h.subtract(&previous) else {
                        rejected += 1;
                        continue;
                    };
                    h = delta;
                }
                sqlx::query("INSERT INTO cumulative VALUES (?, ?, ?, ?, ?) ON CONFLICT(stream, start) DO UPDATE SET end=excluded.end, histogram=excluded.histogram, updated_ms=excluded.updated_ms")
                    .bind(&point.stream).bind(&start).bind(&end).bind(serde_json::to_string(&point.histogram)?).bind(now)
                    .execute(&mut *tx).await?;
                // The first cumulative export may include pre-monitoring history.
                if baseline_only {
                    continue;
                }
            }
            if h.count == 0 {
                continue;
            }
            accepted += sqlx::query("INSERT OR IGNORE INTO samples VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(&point.stream)
                .bind(&start)
                .bind(&end)
                .bind(time)
                .bind(&point.model)
                .bind(&point.kind)
                .bind(serde_json::to_string(&h)?)
                .execute(&mut *tx)
                .await?
                .rows_affected() as usize;
        }
        sqlx::query("INSERT INTO receiver VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET last_received_ms=excluded.last_received_ms")
            .bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok((accepted, rejected))
    }

    pub async fn snapshot(
        &self,
        minutes: u32,
        selected: Option<String>,
        endpoint: String,
        now: i64,
    ) -> Result<Snapshot> {
        self.prune(now).await?;
        let since = now - i64::from(minutes) * 60_000;
        let rows = sqlx::query("SELECT time_ms, model, kind, histogram FROM samples WHERE time_ms >= ? AND time_ms <= ? ORDER BY time_ms, model")
            .bind(since).bind(now + 60_000).fetch_all(&self.pool).await?;
        // With no explicit choice, monitor the most recently observed model.
        let selected = selected.or_else(|| rows.last().and_then(|row| row.try_get("model").ok()));
        let mut models: BTreeMap<String, SummaryBuilder> = BTreeMap::new();
        let mut summary = SummaryBuilder::default();
        let mut trends: BTreeMap<i64, SummaryBuilder> = BTreeMap::new();
        let step = (i64::from(minutes) * 60_000 / 60).max(1000);
        for row in rows {
            let time: i64 = row.try_get("time_ms")?;
            let model: String = row.try_get("model")?;
            let kind: String = row.try_get("kind")?;
            let histogram: Histogram = serde_json::from_str(row.try_get::<&str, _>("histogram")?)?;
            models
                .entry(model.clone())
                .or_default()
                .add(&kind, histogram.clone(), time);
            if selected.as_ref() != Some(&model) {
                continue;
            }
            summary.add(&kind, histogram.clone(), time);
            trends
                .entry(time / step * step)
                .or_default()
                .add(&kind, histogram.clone(), time);
        }
        let last_received_ms: Option<i64> =
            sqlx::query_scalar("SELECT last_received_ms FROM receiver WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;
        let trend = (0..=60)
            .map(|i| {
                let time = since / step * step + i * step;
                let value = trends
                    .get(&time)
                    .map(SummaryBuilder::finish)
                    .unwrap_or_default();
                TrendPoint {
                    time_ms: time,
                    ttft_ms: value.ttft.mean_ms,
                    decode_tps: value.decode_tps,
                    input_tokens: value.input_tokens,
                    cached_input_tokens: value.cached_input_tokens,
                    output_tokens: value.output_tokens,
                    reasoning_output_tokens: value.reasoning_output_tokens,
                    http_failure_percent: value.http_attempts.map(|m| m.failure_percent),
                    websocket_send_failure_percent: value
                        .websocket_send_attempts
                        .map(|m| m.failure_percent),
                }
            })
            .collect();
        Ok(Snapshot {
            now_ms: now,
            last_received_ms,
            window_minutes: minutes,
            endpoint,
            selected_model: selected,
            models: models
                .into_iter()
                .map(|(model, s)| ModelSummary {
                    model,
                    summary: s.finish(),
                })
                .collect(),
            summary: summary.finish(),
            trend,
        })
    }
}
