use crate::{
    metrics::{self, Histogram, Parsed, Point},
    server::{self, AppState},
    store::Store,
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use prost::Message;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

fn export(now: i64, model: &str, count: u64, sum: f64, temporality: u32) -> Value {
    json!({"resourceMetrics":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"codex"}}]},"scopeMetrics":[{"metrics":[{
        "name":metrics::TTFT,"histogram":{"aggregationTemporality":temporality,"dataPoints":[{
            "attributes":[{"key":"model","value":{"stringValue":model}}],
            "startTimeUnixNano":((now-1000) as u64*1_000_000).to_string(),"timeUnixNano":(now as u64*1_000_000).to_string(),
            "count":count.to_string(),"sum":sum,"explicitBounds":[1000.0,5000.0],"bucketCounts":["0",count.to_string(),"0"]
        }]}
    }]}]}]})
}
fn parsed(value: Value) -> Parsed {
    metrics::parse(metrics::decode_json(&serde_json::to_vec(&value).unwrap()).unwrap())
}
fn point(now: i64, start: u64, count: u64, sum: f64, cumulative: bool) -> Point {
    Point {
        stream: "test-stream".into(),
        model: "gpt-test".into(),
        kind: "ttft".into(),
        start,
        end: now as u64 * 1_000_000,
        cumulative,
        histogram: Histogram {
            count,
            sum,
            bounds: vec![1000.0, 5000.0],
            buckets: vec![0, count, 0],
        },
    }
}
fn batch(point: Point) -> Parsed {
    Parsed {
        points: vec![point],
        rejected: 0,
    }
}

#[tokio::test]
async fn delta_duplicates_restart_and_missing_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = Store::open(&path).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let value = export(now, "gpt-test", 1, 3306.0, 1);
    assert_eq!(
        store.ingest(parsed(value.clone()), now).await.unwrap(),
        (1, 0)
    );
    assert_eq!(
        store.ingest(parsed(value.clone()), now).await.unwrap(),
        (0, 0)
    );
    drop(store);
    let store = Store::open(&path).await.unwrap();
    assert_eq!(store.ingest(parsed(value), now).await.unwrap(), (0, 0));
    let snap = store.snapshot(60, None, String::new(), now).await.unwrap();
    assert_eq!(snap.summary.ttft.samples, 1);
    assert_eq!(snap.summary.ttft.mean_ms, Some(3306.0));
    assert_eq!(snap.summary.ttft.p50_ms, Some(3306.0));
    assert_eq!(snap.summary.ttft.p95_ms, None);
    assert_eq!(snap.summary.decode_tps, None);
    assert_eq!(snap.summary.input_tokens, None);
    assert_eq!(snap.models.len(), 1);
}

#[tokio::test]
async fn cumulative_baselines_out_of_order_and_separate_processes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let start = (now - 10_000) as u64 * 1_000_000;
    assert_eq!(
        store
            .ingest(batch(point(now, start, 2, 6000.0, true)), now)
            .await
            .unwrap(),
        (0, 0)
    );
    assert_eq!(
        store
            .ingest(batch(point(now + 1, start, 3, 8000.0, true)), now + 1)
            .await
            .unwrap(),
        (1, 0)
    );
    assert_eq!(
        store
            .ingest(batch(point(now, start, 2, 6000.0, true)), now + 1)
            .await
            .unwrap(),
        (0, 0)
    );
    assert_eq!(
        store
            .ingest(batch(point(now + 2, start + 1, 1, 3000.0, true)), now + 2)
            .await
            .unwrap(),
        (0, 0)
    );
    assert_eq!(
        store
            .ingest(batch(point(now + 3, start + 1, 2, 7000.0, true)), now + 3)
            .await
            .unwrap(),
        (1, 0)
    );
    let snap = store
        .snapshot(60, None, String::new(), now + 3)
        .await
        .unwrap();
    assert_eq!(snap.summary.ttft.samples, 2);
    assert_eq!(snap.summary.ttft.mean_ms, Some(3000.0));
}

#[tokio::test]
async fn independent_tokens_weighted_tbt_filter_and_retention() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let mut tbt = point(now, 1, 2, 100.0, false);
    tbt.kind = "tbt".into();
    tbt.stream = "tbt1".into();
    let mut tbt2 = point(now + 1, 2, 1, 100.0, false);
    tbt2.kind = "tbt".into();
    tbt2.stream = "tbt2".into();
    let mut input = point(now, 1, 1, 12000.0, false);
    input.kind = "input".into();
    input.stream = "input".into();
    let mut cached = point(now, 1, 1, 11000.0, false);
    cached.kind = "cached_input".into();
    cached.stream = "cached".into();
    store
        .ingest(
            Parsed {
                points: vec![tbt, tbt2, input, cached],
                rejected: 0,
            },
            now + 1,
        )
        .await
        .unwrap();
    let snap = store
        .snapshot(60, None, String::new(), now + 1)
        .await
        .unwrap();
    assert_eq!(snap.summary.decode_tps, Some(15.0));
    assert_eq!(snap.summary.input_tokens, Some(12000.0));
    assert_eq!(snap.summary.cached_input_tokens, Some(11000.0));
    assert_eq!(snap.summary.output_tokens, None);
    assert_eq!(
        snap.trend
            .iter()
            .filter_map(|p| p.input_tokens)
            .sum::<f64>(),
        12000.0
    );
    assert_eq!(
        snap.trend
            .iter()
            .filter_map(|p| p.cached_input_tokens)
            .sum::<f64>(),
        11000.0
    );
    assert!(snap.trend.iter().all(|p| p.output_tokens.is_none()));
    let snap = store
        .snapshot(60, Some("another-model".into()), String::new(), now + 1)
        .await
        .unwrap();
    assert_eq!(snap.summary.tbt.samples, 0);
    assert_eq!(snap.models.len(), 1);
    let snap = store
        .snapshot(10080, None, String::new(), now + 8 * 86400000)
        .await
        .unwrap();
    assert!(snap.models.is_empty());
}

#[test]
fn official_otlp_json_and_protobuf_decode_and_validation() {
    let now = chrono::Utc::now().timestamp_millis();
    let value = export(now, "gpt-test", 1, 3306.0, 1);
    let request: ExportMetricsServiceRequest = serde_json::from_value(value.clone()).unwrap();
    let decoded = ExportMetricsServiceRequest::decode(request.encode_to_vec().as_slice()).unwrap();
    assert_eq!(metrics::parse(decoded).points.len(), 1);
    let mut bad = value.clone();
    bad["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["histogram"]["dataPoints"][0]["bucketCounts"] =
        json!(["0", "2", "0"]);
    assert_eq!(parsed(bad).rejected, 1);
    let mut absent = value;
    absent["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["histogram"]["dataPoints"][0]["flags"] =
        json!(1);
    assert!(parsed(absent).points.is_empty());
}

#[tokio::test]
async fn histogram_quantiles_are_marked_approximate() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    store
        .ingest(parsed(export(now, "gpt-test", 20, 60000.0, 1)), now)
        .await
        .unwrap();
    let snap = store.snapshot(60, None, String::new(), now).await.unwrap();
    assert!(snap.summary.ttft.quantiles_approximate);
    assert_eq!(snap.summary.ttft.p50_ms, Some(3000.0));
    assert_eq!(snap.summary.ttft.p95_ms, Some(4800.0));
}

#[tokio::test]
async fn http_ingestion_embedded_page_and_origin_guard() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let (updates, _) = tokio::sync::watch::channel(Some(0));
    let state = AppState {
        store: Arc::new(store),
        updates,
        endpoint: "http://127.0.0.1:4318/v1/metrics".into(),
    };
    let app = server::router(state);
    let request = Request::builder()
        .method("POST")
        .uri("/v1/metrics")
        .header("host", "127.0.0.1:4318")
        .header("content-type", "application/json")
        .body(Body::from(
            export(now, "gpt-test", 1, 3306.0, 1).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let request = Request::builder()
        .uri("/api/snapshot")
        .header("host", "127.0.0.1:4318")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let body = to_bytes(response.into_body(), 1000000).await.unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["summary"]["ttft"]["mean_ms"], 3306.0);
    let request = Request::builder()
        .uri("/api/snapshot")
        .header("host", "192.168.1.10:4318")
        .header("origin", "http://192.168.1.10:4318")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1000000).await.unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["endpoint"], "http://192.168.1.10:4318/v1/metrics");
    #[cfg(not(feature = "embedded-web"))]
    {
        let request = Request::builder()
            .uri("/api/snapshot")
            .header("host", "192.168.1.10:5173")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let body = to_bytes(response.into_body(), 1000000).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["endpoint"], "http://192.168.1.10:4318/v1/metrics");
    }
    let request = Request::builder()
        .uri("/")
        .header("host", "127.0.0.1:4318")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    #[cfg(feature = "embedded-web")]
    {
        assert_eq!(response.status(), StatusCode::OK);
        let html = to_bytes(response.into_body(), 1000000).await.unwrap();
        assert!(String::from_utf8_lossy(&html).contains("Codex Speed"));
    }
    #[cfg(not(feature = "embedded-web"))]
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let request = Request::builder()
        .uri("/api/snapshot")
        .header("host", "192.168.1.10:4318")
        .header("origin", "http://attacker.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn shutdown_ends_sse_connections() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let (updates, _) = tokio::sync::watch::channel(Some(0));
    let app = server::router(AppState {
        store: Arc::new(store),
        updates: updates.clone(),
        endpoint: "http://127.0.0.1:4318/v1/metrics".into(),
    });
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .header("host", "127.0.0.1:4318")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    updates.send_replace(None);
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        to_bytes(response.into_body(), 10000),
    )
    .await
    .expect("SSE should end on shutdown")
    .unwrap();
}

#[tokio::test]
async fn default_snapshot_selects_one_model_without_combining_metrics() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    store
        .ingest(parsed(export(now - 1000, "older", 1, 2000.0, 1)), now)
        .await
        .unwrap();
    store
        .ingest(parsed(export(now, "latest", 1, 4000.0, 1)), now)
        .await
        .unwrap();
    let snap = store.snapshot(60, None, String::new(), now).await.unwrap();
    assert_eq!(snap.selected_model.as_deref(), Some("latest"));
    assert_eq!(snap.summary.ttft.samples, 1);
    assert_eq!(snap.summary.ttft.mean_ms, Some(4000.0));
    assert_eq!(snap.models.len(), 2);
    let snap = store
        .snapshot(60, Some("older".into()), String::new(), now)
        .await
        .unwrap();
    assert_eq!(snap.summary.ttft.mean_ms, Some(2000.0));
}

#[tokio::test]
async fn failed_database_is_recreated_without_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metrics.sqlite");
    std::fs::write(&path, b"not a SQLite database").unwrap();
    let store = Store::open(&path).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    assert!(
        store
            .snapshot(60, None, String::new(), now)
            .await
            .unwrap()
            .models
            .is_empty()
    );
    drop(store);
    let old = sqlx::SqlitePool::connect(&format!("sqlite:{}", path.display()))
        .await
        .unwrap();
    sqlx::raw_sql("DROP TABLE receiver; CREATE TABLE receiver (obsolete INTEGER);")
        .execute(&old)
        .await
        .unwrap();
    old.close().await;
    let store = Store::open(&path).await.unwrap();
    store
        .ingest(parsed(export(now, "new", 1, 2000.0, 1)), now)
        .await
        .unwrap();
    let snap = store.snapshot(60, None, String::new(), now).await.unwrap();
    assert_eq!(snap.summary.ttft.samples, 1);
}

fn counter_export(now: i64, name: &str, success: Value, value: Value, temporality: u32) -> Value {
    json!({"resourceMetrics":[{"scopeMetrics":[{"metrics":[{
        "name":name,"sum":{"isMonotonic":true,"aggregationTemporality":temporality,"dataPoints":[{
            "attributes":[{"key":"model","value":{"stringValue":"gpt-reliability"}},{"key":"success","value":success}],
            "startTimeUnixNano":((now-60_000) as u64*1_000_000).to_string(),
            "timeUnixNano":(now as u64*1_000_000).to_string(),
            "asInt":value
        }]}
    }]}]}]})
}

#[tokio::test]
async fn attempt_counters_are_separate_exact_deduplicated_and_missing_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let empty = store.snapshot(60, None, String::new(), now).await.unwrap();
    assert!(empty.summary.http_attempts.is_none());
    assert!(empty.summary.websocket_send_attempts.is_none());
    for (name, success, value) in [
        ("codex.api_request", true, 9),
        ("codex.api_request", false, 1),
        ("codex.websocket.request", true, 3),
        ("codex.websocket.request", false, 1),
    ] {
        let data = counter_export(
            now,
            name,
            json!({"stringValue":success.to_string()}),
            json!(value.to_string()),
            1,
        );
        assert_eq!(
            store.ingest(parsed(data.clone()), now).await.unwrap(),
            (1, 0)
        );
        assert_eq!(store.ingest(parsed(data), now).await.unwrap(), (0, 0));
    }
    let snap = store.snapshot(60, None, String::new(), now).await.unwrap();
    let http = snap.summary.http_attempts.unwrap();
    assert_eq!(
        (http.total, http.failed, http.failure_percent),
        (10, 1, 10.0)
    );
    let ws = snap.summary.websocket_send_attempts.unwrap();
    assert_eq!((ws.total, ws.failed, ws.failure_percent), (4, 1, 25.0));
    assert_eq!(
        snap.trend
            .iter()
            .filter(|p| p.http_failure_percent.is_some())
            .count(),
        1
    );
    assert!(snap.summary.ttft.mean_ms.is_none());

    // All successful attempts produce a real zero failure rate.
    let next = now + 60_000;
    let ok = counter_export(
        next,
        "codex.api_request",
        json!({"boolValue":true}),
        json!(2),
        1,
    );
    assert_eq!(store.ingest(parsed(ok), next).await.unwrap(), (1, 0));
    let snap = store.snapshot(60, None, String::new(), next).await.unwrap();
    assert!(
        snap.trend
            .iter()
            .any(|p| p.http_failure_percent == Some(0.0))
    );
    let invalid = counter_export(
        next,
        "codex.api_request",
        json!({"stringValue":"unknown"}),
        json!(1),
        1,
    );
    assert_eq!(parsed(invalid).rejected, 1);
    let invalid = counter_export(
        next,
        "codex.api_request",
        json!({"boolValue":true}),
        json!(-1),
        1,
    );
    assert_eq!(parsed(invalid).rejected, 1);
}

#[tokio::test]
async fn cumulative_attempt_counters_use_baselines_and_restart_identity() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let first = counter_export(
        now,
        "codex.websocket.request",
        json!({"boolValue":false}),
        json!(8),
        2,
    );
    assert_eq!(
        store.ingest(parsed(first.clone()), now).await.unwrap(),
        (0, 0)
    );
    let mut next = first.clone();
    let point =
        &mut next["resourceMetrics"][0]["scopeMetrics"][0]["metrics"][0]["sum"]["dataPoints"][0];
    point["timeUnixNano"] = json!(((now + 1) as u64 * 1_000_000).to_string());
    point["asInt"] = json!(10);
    assert_eq!(
        store.ingest(parsed(next.clone()), now + 1).await.unwrap(),
        (1, 0)
    );
    assert_eq!(store.ingest(parsed(next), now + 1).await.unwrap(), (0, 0));
    let snap = store
        .snapshot(60, None, String::new(), now + 1)
        .await
        .unwrap();
    assert_eq!(snap.summary.websocket_send_attempts.unwrap().failed, 2);
    assert_eq!(store.ingest(parsed(first), now + 1).await.unwrap(), (0, 0));
}

#[tokio::test]
async fn cache_share_requires_complete_counts_and_nonzero_input() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.sqlite")).await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    for (model, input_count, input_sum, cached_count, cached_sum) in [
        ("paired", 2, 200.0, 2, 150.0),
        ("partial", 2, 200.0, 1, 150.0),
        ("zero", 1, 0.0, 1, 0.0),
        ("invalid_share", 1, 10.0, 1, 20.0),
    ] {
        for (kind, count, sum) in [
            ("input", input_count, input_sum),
            ("cached_input", cached_count, cached_sum),
        ] {
            let mut p = point(now, 1, count, sum, false);
            p.model = model.into();
            p.kind = kind.into();
            p.stream = format!("{model}-{kind}");
            store.ingest(batch(p), now).await.unwrap();
        }
        let snap = store
            .snapshot(60, Some(model.into()), String::new(), now)
            .await
            .unwrap();
        assert_eq!(
            snap.summary.cached_input_percent,
            if model == "paired" { Some(75.0) } else { None }
        );
    }
}
