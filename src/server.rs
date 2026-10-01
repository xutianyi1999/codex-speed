use crate::{
    metrics,
    store::{Snapshot, Store},
};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsPartialSuccess, ExportMetricsServiceRequest, ExportMetricsServiceResponse,
};
use prost::Message;
use rust_embed::Embed;
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};

#[derive(Embed)]
#[folder = "web/dist/"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub updates: watch::Sender<u64>,
    pub endpoint: String,
}

#[derive(Debug)]
struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error": self.1}))).into_response()
    }
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    eprintln!("Metrics storage error: {error}");
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Metrics storage unavailable",
    )
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/metrics", post(ingest))
        .route("/api/snapshot", get(snapshot))
        .route("/api/events", get(events))
        .route(
            "/api/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .fallback(get(asset))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            local_request,
        ))
        .with_state(state)
}

async fn local_request(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let base = state.endpoint.trim_end_matches("/v1/metrics");
    let host = base.trim_start_matches("http://");
    let localhost = host.replacen("127.0.0.1", "localhost", 1);
    if request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_none_or(|h| h != host && h != localhost)
    {
        return (StatusCode::FORBIDDEN, "Use the local dashboard address").into_response();
    }
    if request
        .headers()
        .get(header::ORIGIN)
        .is_some_and(|origin| origin != base && origin != format!("http://{localhost}").as_str())
    {
        return (
            StatusCode::FORBIDDEN,
            "Cross-origin requests are not accepted",
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::CONTENT_SECURITY_POLICY, "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'".parse().unwrap());
    response
}

async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    let json = match content_type {
        "application/json" => true,
        "application/x-protobuf" => false,
        _ => {
            return Err(ApiError(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Use OTLP JSON or protobuf",
            ));
        }
    };
    let request: ExportMetricsServiceRequest = if json {
        serde_json::from_slice(&body)
            .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid OTLP JSON"))?
    } else {
        Message::decode(body)
            .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid OTLP protobuf"))?
    };
    let parsed = metrics::parse(request);
    let (_, rejected) = state
        .store
        .ingest(parsed, chrono::Utc::now().timestamp_millis())
        .await
        .map_err(internal)?;
    // Even a batch with no supported measurements confirms the exporter is connected.
    state
        .updates
        .send_modify(|revision| *revision = revision.wrapping_add(1));
    let response = ExportMetricsServiceResponse {
        partial_success: (rejected > 0).then(|| ExportMetricsPartialSuccess {
            rejected_data_points: rejected,
            error_message: "Some supported metrics had invalid or unsupported data points".into(),
        }),
    };
    if json {
        // Official response type; omit null partialSuccess for a full success.
        if rejected == 0 {
            return Ok(Json(serde_json::json!({})).into_response());
        }
        Ok(Json(response).into_response())
    } else {
        Ok((
            [(header::CONTENT_TYPE, "application/x-protobuf")],
            response.encode_to_vec(),
        )
            .into_response())
    }
}

#[derive(Deserialize)]
struct Filter {
    #[serde(default = "default_minutes")]
    minutes: u32,
    model: Option<String>,
}

fn default_minutes() -> u32 {
    60
}

async fn snapshot(
    State(state): State<AppState>,
    Query(filter): Query<Filter>,
) -> Result<Response, ApiError> {
    if ![15, 60, 1440, 10080].contains(&filter.minutes)
        || filter.model.as_ref().is_some_and(|m| m.len() > 256)
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Invalid monitoring window or model",
        ));
    }
    let result: Snapshot = state
        .store
        .snapshot(
            filter.minutes,
            filter.model,
            state.endpoint,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .map_err(internal)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}

async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let stream = WatchStream::new(state.updates.subscribe()).map(|revision| {
        Ok::<_, Infallible>(Event::default().event("metrics").data(revision.to_string()))
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
}

async fn asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let Some(file) = Assets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            mime_guess::from_path(path).first_or_octet_stream().as_ref(),
        )
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(file.data.into_owned()))
        .unwrap()
}
