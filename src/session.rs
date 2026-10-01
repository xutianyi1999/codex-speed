use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
};

pub const HISTORY_LIMIT: usize = 100;

#[derive(Deserialize)]
struct LogRecord<'a> {
    #[serde(borrow)]
    timestamp: Option<&'a str>,
    #[serde(rename = "type", default, borrow)]
    kind: &'a str,
    #[serde(borrow)]
    payload: Option<&'a serde_json::value::RawValue>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub output_tokens: u64,
    #[serde(default)]
    pub reasoning_output_tokens: Option<u64>,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            output_tokens: 0,
            reasoning_output_tokens: Some(0),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Turn {
    pub id: String,
    pub model: String,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub first_output_ms: Option<u64>,
    pub usage: Option<Usage>,
    #[serde(skip)]
    response_ids: HashSet<String>,
    #[serde(skip)]
    legacy_baseline: Usage,
    #[serde(skip)]
    exact_usage: bool,
}

impl Turn {
    pub fn average_tps(&self) -> Option<f64> {
        let ms = self.duration_ms.filter(|ms| *ms > 0)?;
        Some(self.usage.as_ref()?.output_tokens as f64 * 1000.0 / ms as f64)
    }
    pub fn visible_tps(&self) -> Option<f64> {
        let ms = self.duration_ms.filter(|ms| *ms > 0)?;
        let usage = self.usage.as_ref()?;
        Some(
            usage
                .output_tokens
                .checked_sub(usage.reasoning_output_tokens?)? as f64
                * 1000.0
                / ms as f64,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: String,
    pub path: PathBuf,
    pub cwd: String,
    pub source: String,
    pub model: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub turns: VecDeque<Turn>,
    pub malformed_lines: usize,
    pub dropped_turns: usize,
    #[serde(skip)]
    legacy_total: Usage,
}

impl Session {
    pub fn new(path: PathBuf) -> Self {
        Self {
            id: String::new(),
            path,
            cwd: String::new(),
            source: String::new(),
            model: String::new(),
            updated_at: None,
            turns: VecDeque::new(),
            malformed_lines: 0,
            dropped_turns: 0,
            legacy_total: Usage::default(),
        }
    }
    pub fn is_supported_source(&self) -> bool {
        matches!(self.source.as_str(), "cli" | "exec" | "vscode")
    }
    pub fn latest(&self) -> Option<&Turn> {
        self.turns.back()
    }

    fn turn(&mut self, id: &str) -> &mut Turn {
        if let Some(index) = self.turns.iter().position(|t| t.id == id) {
            return &mut self.turns[index];
        }
        if self.turns.len() >= HISTORY_LIMIT {
            self.turns.pop_front();
            self.dropped_turns += 1;
        }
        self.turns.push_back(Turn {
            id: id.to_owned(),
            model: String::new(),
            status: "running".into(),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            first_output_ms: None,
            usage: None,
            response_ids: HashSet::new(),
            legacy_baseline: self.legacy_total.clone(),
            exact_usage: false,
        });
        self.turns.back_mut().unwrap()
    }

    pub fn ingest(&mut self, line: &[u8]) {
        let record: LogRecord<'_> = match serde_json::from_slice(line) {
            Ok(value) => value,
            Err(_) => {
                self.malformed_lines += 1;
                return;
            }
        };
        let timestamp = record
            .timestamp
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        self.updated_at = self.updated_at.max(timestamp);
        // Validate the envelope without allocating strings/trees for large
        // assistant replies, prompts, tool output, or compacted transcripts.
        if !matches!(
            record.kind,
            "session_meta" | "turn_context" | "token_usage_record" | "event_msg"
        ) {
            return;
        }
        if record.kind == "event_msg"
            && let Some(raw) = record.payload
        {
            let event: LogRecord<'_> = match serde_json::from_str(raw.get()) {
                Ok(value) => value,
                Err(_) => {
                    self.malformed_lines += 1;
                    return;
                }
            };
            if !matches!(
                event.kind,
                "task_started"
                    | "turn_started"
                    | "task_complete"
                    | "turn_complete"
                    | "turn_aborted"
                    | "token_count"
            ) {
                return;
            }
        }
        let payload: Value = match record
            .payload
            .map(|p| serde_json::from_str(p.get()))
            .transpose()
        {
            Ok(value) => value.unwrap_or(Value::Null),
            Err(_) => {
                self.malformed_lines += 1;
                return;
            }
        };
        let p = &payload;
        match record.kind {
            "session_meta" => {
                self.id = string(p, "id");
                self.cwd = string(p, "cwd");
                self.source = string(p, "source");
            }
            "turn_context" => {
                self.model = string(p, "model");
                let model = self.model.clone();
                if let Some(id) = p["turn_id"].as_str() {
                    self.turn(id).model = model;
                } else if let Some(turn) = self.turns.back_mut()
                    && turn.status == "running"
                {
                    turn.model = model;
                }
            }
            "token_usage_record" => {
                // Forked transcripts may contain usage belonging to another thread.
                if p["thread_id"].as_str().is_some_and(|id| id != self.id) {
                    return;
                }
                let Some(id) = p["turn_id"].as_str() else {
                    return;
                };
                let Some(response) = p["response_id"].as_str() else {
                    return;
                };
                let usage = serde_json::from_value::<Usage>(p["turn_token_usage"].clone()).ok();
                let turn = self.turn(id);
                if let Some(usage) = usage
                    && turn.response_ids.insert(response.to_owned())
                {
                    turn.usage = Some(usage);
                    turn.exact_usage = true;
                }
            }
            "event_msg" => self.event(p, timestamp),
            _ => {}
        }
    }

    fn event(&mut self, p: &Value, timestamp: Option<DateTime<Utc>>) {
        match p["type"].as_str().unwrap_or_default() {
            "task_started" | "turn_started" => {
                let Some(id) = p["turn_id"].as_str() else {
                    return;
                };
                let model = self.model.clone();
                let turn = self.turn(id);
                // An explicit turn_context is authoritative. A context seen
                // before turn/start also supplies the model for older formats.
                if turn.model.is_empty() {
                    turn.model = model;
                }
                turn.started_at = unix_time(p, "started_at").or(timestamp);
            }
            "task_complete" | "turn_complete" | "turn_aborted" => {
                let id = p["turn_id"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| self.latest().map(|t| t.id.clone()));
                let Some(id) = id else {
                    return;
                };
                let turn = self.turn(&id);
                turn.status = if p["type"] == "turn_aborted" {
                    "interrupted"
                } else if !p["error"].is_null() {
                    "failed"
                } else {
                    "completed"
                }
                .into();
                turn.started_at = unix_time(p, "started_at").or(turn.started_at);
                turn.finished_at = unix_time(p, "completed_at").or(timestamp);
                // Rounded timestamps are insufficient for a meaningful TPS measurement.
                turn.duration_ms = p["duration_ms"].as_u64();
                turn.first_output_ms = p["time_to_first_token_ms"].as_u64();
            }
            "token_count" => {
                let Some(total) = p
                    .pointer("/info/total_token_usage")
                    .and_then(|v| serde_json::from_value::<Usage>(v.clone()).ok())
                else {
                    return;
                };
                if let Some(turn) = self.turns.back_mut()
                    && turn.status == "running"
                    && !turn.exact_usage
                {
                    if total.output_tokens >= turn.legacy_baseline.output_tokens {
                        turn.usage = Some(Usage {
                            output_tokens: total.output_tokens - turn.legacy_baseline.output_tokens,
                            reasoning_output_tokens: total
                                .reasoning_output_tokens
                                .zip(turn.legacy_baseline.reasoning_output_tokens)
                                .and_then(|(total, baseline)| total.checked_sub(baseline)),
                        });
                    } else {
                        turn.usage = None;
                    }
                }
                self.legacy_total = total;
            }
            _ => {}
        }
    }
}

fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_owned()
}
fn unix_time(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(v[key].as_i64()?, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn feed(s: &mut Session, kind: &str, payload: Value) {
        s.ingest(&serde_json::to_vec(&json!({"type": kind, "payload": payload})).unwrap());
    }
    #[test]
    fn deduplicates_usage_and_keeps_reasoning_inside_output_total() {
        let mut s = Session::new("sample.jsonl".into());
        feed(&mut s, "session_meta", json!({"id":"s", "source":"cli"}));
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_started","turn_id":"t"}),
        );
        let usage = json!({"thread_id":"s","turn_id":"t","response_id":"r", "turn_token_usage":{"output_tokens":100,"reasoning_output_tokens":40}});
        feed(&mut s, "token_usage_record", usage.clone());
        feed(&mut s, "token_usage_record", usage);
        feed(
            &mut s,
            "token_usage_record",
            json!({"thread_id":"parent","turn_id":"t","response_id":"other", "turn_token_usage":{"output_tokens":999}}),
        );
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_complete","turn_id":"t","duration_ms":2000,"time_to_first_token_ms":250}),
        );
        let t = s.latest().unwrap();
        assert_eq!(t.average_tps(), Some(50.0));
        assert_eq!(t.visible_tps(), Some(30.0));
        assert_eq!(t.first_output_ms, Some(250));
    }
    #[test]
    fn legacy_cumulative_usage_does_not_double_count_repeated_events() {
        let mut s = Session::new("old.jsonl".into());
        feed(
            &mut s,
            "event_msg",
            json!({"type":"token_count","info":{"total_token_usage":{"output_tokens":200}}}),
        );
        feed(
            &mut s,
            "event_msg",
            json!({"type":"turn_started","turn_id":"t"}),
        );
        for _ in 0..2 {
            feed(
                &mut s,
                "event_msg",
                json!({"type":"token_count","info":{"total_token_usage":{"output_tokens":300}}}),
            );
        }
        feed(
            &mut s,
            "event_msg",
            json!({"type":"turn_complete","turn_id":"t","duration_ms":1000}),
        );
        assert_eq!(s.latest().unwrap().average_tps(), Some(100.0));
        assert_eq!(s.latest().unwrap().first_output_ms, None);
    }
    #[test]
    fn missing_reasoning_usage_preserves_total_speed_but_hides_visible_speed() {
        let mut s = Session::new("sample.jsonl".into());
        feed(&mut s, "session_meta", json!({"id":"s", "source":"cli"}));
        feed(
            &mut s,
            "token_usage_record",
            json!({"thread_id":"s","turn_id":"t","response_id":"r",
            "turn_token_usage":{"output_tokens":100}}),
        );
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_complete","turn_id":"t","duration_ms":1000}),
        );
        assert_eq!(s.latest().unwrap().average_tps(), Some(100.0));
        assert_eq!(s.latest().unwrap().visible_tps(), None);
    }

    #[test]
    fn missing_or_zero_duration_never_invents_speed() {
        let mut s = Session::new("sample.jsonl".into());
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_started","turn_id":"t"}),
        );
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_complete","turn_id":"t","duration_ms":0}),
        );
        assert_eq!(s.latest().unwrap().average_tps(), None);
        s.ingest(b"not json");
        assert_eq!(s.malformed_lines, 1);
    }

    #[test]
    fn skips_large_irrelevant_payloads_but_still_rejects_invalid_json() {
        let mut s = Session::new("sample.jsonl".into());
        for kind in ["response_item", "event_msg"] {
            feed(
                &mut s,
                kind,
                json!({"type":"agent_message", "text":"x".repeat(1_000_000)}),
            );
        }
        assert!(s.turns.is_empty());
        assert_eq!(s.malformed_lines, 0);
        s.ingest(br#"{"type":"response_item","payload":{"text":invalid}}"#);
        assert_eq!(s.malformed_lines, 1);
        feed(
            &mut s,
            "event_msg",
            json!({"type":"task_complete","turn_id":"t","duration_ms":1000,"time_to_first_token_ms":123}),
        );
        assert_eq!(s.latest().unwrap().first_output_ms, Some(123));
    }
}
