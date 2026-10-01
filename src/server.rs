use crate::{
    metrics,
    store::{Snapshot, Store},
};
#[cfg(feature = "embedded-web")]
use axum::body::Body;
use axum::{
    Json, Router,
    body::Bytes,
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
#[cfg(feature = "embedded-web")]
use rust_embed::Embed;
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};

#[cfg(feature = "embedded-web")]
#[derive(Embed)]
#[folder = "web/dist/"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub updates: watch::Sender<Option<u64>>,
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
        .merge(frontend_routes())
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(RequestDecompressionLayer::new())
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn(same_origin_request))
        .with_state(state)
}

async fn same_origin_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
    else {
        return (StatusCode::BAD_REQUEST, "Missing Host header").into_response();
    };
    if host.parse::<axum::http::uri::Authority>().is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid Host header").into_response();
    }
    if request.headers().get(header::ORIGIN).is_some_and(|origin| {
        origin
            .to_str()
            .ok()
            .and_then(|value| value.parse::<Uri>().ok())
            .is_none_or(|uri| {
                !matches!(uri.scheme_str(), Some("http" | "https"))
                    || uri
                        .authority()
                        .is_none_or(|authority| !authority.as_str().eq_ignore_ascii_case(host))
            })
    }) {
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
        metrics::decode_json(&body)
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
    state.updates.send_modify(|revision| {
        if let Some(value) = revision {
            *value = value.wrapping_add(1);
        }
    });
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
    headers: HeaderMap,
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
            headers
                .get(header::HOST)
                .and_then(|h| h.to_str().ok())
                .map(|host| {
                    let scheme = headers
                        .get(header::ORIGIN)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|origin| origin.parse::<Uri>().ok())
                        .and_then(|uri| uri.scheme_str().map(str::to_owned))
                        .unwrap_or_else(|| "http".into());
                    // Vite forwards its Host header in dev. Export directly to the
                    // backend port instead of requiring the frontend proxy to run.
                    #[cfg(not(feature = "embedded-web"))]
                    let host = host
                        .parse::<axum::http::uri::Authority>()
                        .ok()
                        .zip(
                            state
                                .endpoint
                                .parse::<Uri>()
                                .ok()
                                .and_then(|uri| uri.port_u16()),
                        )
                        .map(|(authority, port)| format!("{}:{port}", authority.host()))
                        .unwrap_or_else(|| host.to_owned());
                    format!("{scheme}://{host}/v1/metrics")
                })
                .unwrap_or(state.endpoint),
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .map_err(internal)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}

async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let stream = WatchStream::new(state.updates.subscribe())
        .map_while(|revision| revision)
        .map(|revision| {
            Ok::<_, Infallible>(Event::default().event("metrics").data(revision.to_string()))
        });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10)))
}

fn frontend_routes() -> Router<AppState> {
    #[cfg(feature = "embedded-web")]
    {
        Router::new().fallback(get(asset))
    }
    #[cfg(not(feature = "embedded-web"))]
    {
        Router::new()
    }
}

#[cfg(feature = "embedded-web")]
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
