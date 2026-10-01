use anyhow::{Result, bail};
use opentelemetry_proto::tonic::{
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::{KeyValue, any_value},
    metrics::v1::{AggregationTemporality, HistogramDataPoint, metric},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TTFT: &str = "codex.responses_api_engine_service_ttft.duration_ms";
pub const TBT: &str = "codex.responses_api_engine_service_tbt.duration_ms";
const TOKEN_USAGE: &str = "codex.turn.token_usage";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Histogram {
    pub count: u64,
    pub sum: f64,
    pub bounds: Vec<f64>,
    pub buckets: Vec<u64>,
}

impl Histogram {
    fn from_point(point: &HistogramDataPoint) -> Result<Self> {
        let Some(sum) = point.sum else {
            bail!("Histogram has no sum")
        };
        let histogram = Self {
            count: point.count,
            sum,
            bounds: point.explicit_bounds.clone(),
            buckets: point.bucket_counts.clone(),
        };
        if !sum.is_finite()
            || sum < 0.0
            || point.count > i64::MAX as u64
            || histogram.bounds.iter().any(|v| !v.is_finite() || *v < 0.0)
            || histogram.bounds.windows(2).any(|w| w[0] >= w[1])
            || (histogram.buckets.is_empty() && !histogram.bounds.is_empty())
            || (!histogram.buckets.is_empty()
                && histogram.buckets.len() != histogram.bounds.len() + 1)
            || (!histogram.buckets.is_empty()
                && histogram
                    .buckets
                    .iter()
                    .try_fold(0_u64, |a, b| a.checked_add(*b))
                    != Some(point.count))
            || (point.count == 0 && sum != 0.0)
        {
            bail!("Invalid non-negative histogram")
        }
        Ok(histogram)
    }

    pub fn subtract(&self, previous: &Self) -> Option<Self> {
        if self.bounds != previous.bounds
            || self.buckets.len() != previous.buckets.len()
            || self.sum < previous.sum
        {
            return None;
        }
        Some(Self {
            count: self.count.checked_sub(previous.count)?,
            sum: self.sum - previous.sum,
            bounds: self.bounds.clone(),
            buckets: self
                .buckets
                .iter()
                .zip(&previous.buckets)
                .map(|(a, b)| a.checked_sub(*b))
                .collect::<Option<Vec<_>>>()?,
        })
    }
}

#[derive(Debug)]
pub struct Point {
    pub stream: String,
    pub model: String,
    pub kind: String,
    pub start: u64,
    pub end: u64,
    pub cumulative: bool,
    pub histogram: Histogram,
}

#[derive(Default)]
pub struct Parsed {
    pub points: Vec<Point>,
    pub rejected: i64,
}

fn text<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| a.value.as_ref())
        .and_then(|v| match v.value.as_ref()? {
            any_value::Value::StringValue(s) => Some(s.as_str()),
            _ => None,
        })
}

fn canonical_attributes(attributes: &[KeyValue]) -> Vec<KeyValue> {
    let mut sorted = attributes.to_vec();
    sorted.sort_by(|a, b| a.key.cmp(&b.key));
    sorted
}

pub fn parse(request: ExportMetricsServiceRequest) -> Parsed {
    let mut result = Parsed::default();
    for resource in request.resource_metrics {
        let resource_attributes = resource
            .resource
            .as_ref()
            .map(|r| canonical_attributes(&r.attributes))
            .unwrap_or_default();
        for scope in resource.scope_metrics {
            for metric in scope.metrics {
                let kind = match metric.name.as_str() {
                    TTFT => "ttft",
                    TBT => "tbt",
                    "codex.responses_api_engine_iapi_ttft.duration_ms" => "iapi_ttft",
                    "codex.responses_api_engine_iapi_tbt.duration_ms" => "iapi_tbt",
                    "codex.responses_api_inference_time.duration_ms" => "engine",
                    "codex.responses_api_overhead.duration_ms" => "overhead",
                    TOKEN_USAGE => "tokens",
                    _ => continue,
                };
                let Some(metric::Data::Histogram(histogram)) = metric.data else {
                    result.rejected += 1;
                    continue;
                };
                let temporality =
                    AggregationTemporality::try_from(histogram.aggregation_temporality);
                for point in histogram.data_points {
                    let actual_kind = if kind == "tokens" {
                        match text(&point.attributes, "token_type") {
                            Some("input") => "input",
                            Some("cached_input") => "cached_input",
                            Some("output") => "output",
                            Some("reasoning_output") => "reasoning_output",
                            _ => continue,
                        }
                    } else {
                        kind
                    };
                    // NO_RECORDED_VALUE isn't a zero observation.
                    if point.flags & 1 != 0 {
                        continue;
                    }
                    let valid = Histogram::from_point(&point);
                    let model = text(&point.attributes, "model")
                        .or_else(|| text(&resource_attributes, "model"));
                    if valid.is_err()
                        || (kind == "tokens"
                            && valid.as_ref().is_ok_and(|h| {
                                h.sum.fract() != 0.0 || h.sum > 9_007_199_254_740_991.0
                            }))
                        || model.is_none_or(|m| m.is_empty() || m.len() > 256)
                        || point.start_time_unix_nano == 0
                        || point.time_unix_nano < point.start_time_unix_nano
                        || !matches!(
                            temporality,
                            Ok(AggregationTemporality::Delta | AggregationTemporality::Cumulative)
                        )
                    {
                        result.rejected += 1;
                        continue;
                    }
                    // Keep only a hash of stream metadata: no account, prompt, or raw resource data on disk.
                    let identity = serde_json::to_vec(&(
                        &resource_attributes,
                        &scope.scope,
                        &metric.name,
                        &metric.unit,
                        canonical_attributes(&point.attributes),
                    ))
                    .expect("OTLP attributes are serializable");
                    let stream = const_hex::encode(Sha256::digest(identity));
                    result.points.push(Point {
                        stream,
                        model: model.unwrap().to_owned(),
                        kind: actual_kind.to_owned(),
                        start: point.start_time_unix_nano,
                        end: point.time_unix_nano,
                        cumulative: temporality == Ok(AggregationTemporality::Cumulative),
                        histogram: valid.unwrap(),
                    });
                }
            }
        }
    }
    result
}
