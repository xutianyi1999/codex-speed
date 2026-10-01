use crate::session::{Session, Turn};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct ModelStats<'a> {
    pub model: String,
    pub completed: usize,
    pub unfinished: usize,
    pub excluded: usize,
    pub failed: usize,
    pub interrupted: usize,
    pub latest_completed_at: Option<DateTime<Utc>>,
    pub latest_output_tps: Option<f64>,
    pub latest_first_output_ms: Option<u64>,
    pub tps_samples: usize,
    pub visible_tps_samples: usize,
    pub latency_samples: usize,
    pub output_tps_p50: Option<f64>,
    pub visible_tps_p50: Option<f64>,
    pub first_output_p50_ms: Option<f64>,
    pub first_output_p95_ms: Option<f64>,
    #[serde(skip)]
    pub turns: Vec<&'a Turn>,
}

pub fn summarize<'a>(
    sessions: &[&'a Session],
    since: Option<DateTime<Utc>>,
) -> Vec<ModelStats<'a>> {
    let mut groups: BTreeMap<String, Vec<&Turn>> = BTreeMap::new();
    for session in sessions {
        for turn in &session.turns {
            if since.is_some_and(|since| {
                turn.finished_at
                    .or(turn.started_at)
                    .is_none_or(|time| time < since)
            }) {
                continue;
            }
            let model = if turn.model.is_empty() {
                "unknown"
            } else {
                &turn.model
            };
            groups.entry(model.to_owned()).or_default().push(turn);
        }
    }
    groups
        .into_iter()
        .map(|(model, mut turns)| {
            turns.sort_by(|a, b| {
                b.finished_at
                    .or(b.started_at)
                    .cmp(&a.finished_at.or(a.started_at))
            });
            let completed: Vec<_> = turns.iter().filter(|t| t.status == "completed").collect();
            let mut output: Vec<_> = completed.iter().filter_map(|t| t.average_tps()).collect();
            let mut visible: Vec<_> = completed.iter().filter_map(|t| t.visible_tps()).collect();
            let mut latency: Vec<_> = completed
                .iter()
                .filter_map(|t| t.first_output_ms.map(|v| v as f64))
                .collect();
            for values in [&mut output, &mut visible, &mut latency] {
                values.sort_by(f64::total_cmp);
            }
            let latest = completed.first();
            ModelStats {
                failed: turns.iter().filter(|t| t.status == "failed").count(),
                interrupted: turns.iter().filter(|t| t.status == "interrupted").count(),
                latest_completed_at: latest.and_then(|t| t.finished_at),
                latest_output_tps: latest.and_then(|t| t.average_tps()),
                latest_first_output_ms: latest.and_then(|t| t.first_output_ms),
                completed: completed.len(),
                unfinished: turns.iter().filter(|t| t.status == "running").count(),
                excluded: turns
                    .iter()
                    .filter(|t| matches!(t.status.as_str(), "interrupted" | "failed"))
                    .count(),
                tps_samples: output.len(),
                visible_tps_samples: visible.len(),
                latency_samples: latency.len(),
                output_tps_p50: median(&output),
                visible_tps_p50: median(&visible),
                first_output_p50_ms: median(&latency),
                first_output_p95_ms: nearest_rank(&latency, 0.95),
                model,
                turns,
            }
        })
        .collect()
}

fn median(sorted: &[f64]) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let middle = sorted.len() / 2;
    Some(if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    })
}
fn nearest_rank(sorted: &[f64], quantile: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    sorted
        .get((quantile * sorted.len() as f64).ceil() as usize - 1)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn feed(session: &mut Session, kind: &str, payload: serde_json::Value) {
        session.ingest(
            &serde_json::to_vec(
                &json!({"timestamp":"2026-10-01T06:30:00Z","type":kind,"payload":payload}),
            )
            .unwrap(),
        );
    }
    #[test]
    fn model_switches_do_not_relabel_history_or_include_interrupted_turns() {
        let mut s = Session::new("sample.jsonl".into());
        feed(&mut s, "session_meta", json!({"id":"s","source":"cli"}));
        for (id, model, tokens, latency, kind) in [
            ("a", "model-a", 100, 100, "task_complete"),
            ("b", "model-b", 900, 900, "task_complete"),
            ("c", "model-a", 300, 300, "task_complete"),
            ("d", "model-a", 99000, 9999, "turn_aborted"),
        ] {
            feed(
                &mut s,
                "event_msg",
                json!({"type":"task_started","turn_id":id}),
            );
            feed(&mut s, "turn_context", json!({"turn_id":id,"model":model}));
            feed(
                &mut s,
                "token_usage_record",
                json!({"thread_id":"s","turn_id":id,"response_id":id,
                "turn_token_usage":{"output_tokens":tokens,"reasoning_output_tokens":0}}),
            );
            feed(
                &mut s,
                "event_msg",
                json!({"type":kind,"turn_id":id,"duration_ms":1000,"time_to_first_token_ms":latency}),
            );
        }
        let models = summarize(&[&s], None);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model, "model-a");
        assert_eq!(models[0].completed, 2);
        assert_eq!(models[0].excluded, 1);
        assert_eq!(models[0].output_tps_p50, Some(200.0));
        assert_eq!(models[0].first_output_p50_ms, Some(200.0));
        assert_eq!(models[0].first_output_p95_ms, Some(300.0));
        assert_eq!(models[1].output_tps_p50, Some(900.0));
        let since = DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(summarize(&[&s], Some(since)).is_empty());
    }
    #[test]
    fn missing_samples_remain_unknown() {
        let mut s = Session::new("old.jsonl".into());
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_complete","turn_id":"t"}),
        );
        let models = summarize(&[&s], None);
        assert_eq!(models[0].model, "unknown");
        assert_eq!(models[0].completed, 1);
        assert_eq!(models[0].latency_samples, 0);
        assert_eq!(models[0].output_tps_p50, None);
    }

    #[test]
    fn latest_sample_is_completed_not_the_newer_failed_or_running_turn() {
        let mut s = Session::new("sample.jsonl".into());
        for (id, status, timestamp) in [
            ("done", "task_complete", "2026-10-01T06:00:00Z"),
            ("failed", "task_complete", "2026-10-01T07:00:00Z"),
            ("open", "task_started", "2026-10-01T08:00:00Z"),
        ] {
            feed(&mut s, "turn_context", json!({"turn_id":id,"model":"a"}));
            s.ingest(&serde_json::to_vec(&json!({"timestamp":timestamp,"type":"event_msg",
                "payload":{"type":status,"turn_id":id,"duration_ms":1000,
                    "time_to_first_token_ms":123,"error":if id == "failed" {json!("error")} else {json!(null)}}})).unwrap());
        }
        let stats = summarize(&[&s], None);
        assert_eq!(stats[0].completed, 1);
        assert_eq!(stats[0].failed, 1);
        assert_eq!(stats[0].unfinished, 1);
        assert_eq!(stats[0].latest_first_output_ms, Some(123));
        assert_eq!(
            stats[0].latest_completed_at.unwrap().to_rfc3339(),
            "2026-10-01T06:00:00+00:00"
        );
        assert_eq!(stats[0].latest_output_tps, None);
    }
}
