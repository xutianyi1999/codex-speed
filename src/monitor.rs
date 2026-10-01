use crate::session::Session;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, Metadata},
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};
use walkdir::WalkDir;

struct Tail {
    offset: u64,
    modified: Option<SystemTime>,
    identity: Option<(u64, u64)>,
    session: Session,
}

impl Tail {
    fn new(path: PathBuf) -> Self {
        Self {
            offset: 0,
            modified: None,
            identity: None,
            session: Session::new(path),
        }
    }
    fn read(&mut self) -> Result<()> {
        let file = File::open(&self.session.path)?;
        let metadata = file.metadata()?;
        let identity = file_identity(&metadata);
        let modified = metadata.modified().ok();
        if self.identity != identity
            || metadata.len() < self.offset
            || (metadata.len() == self.offset
                && self.modified.is_some()
                && self.modified != modified)
        {
            let path = self.session.path.clone();
            *self = Self::new(path);
        }
        self.identity = identity;
        self.modified = modified;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(self.offset))?;
        let mut line = Vec::new();
        loop {
            line.clear();
            let bytes = reader.read_until(b'\n', &mut line)?;
            if bytes == 0 || line.last() != Some(&b'\n') {
                break;
            }
            // A writer may still be appending the final line. Only advance over
            // complete lines, so UTF-8 and JSON splits survive the next refresh.
            self.offset += bytes as u64;
            if !line.iter().all(u8::is_ascii_whitespace) {
                self.session.ingest(&line);
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn file_identity(metadata: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}
#[cfg(not(unix))]
fn file_identity(_: &Metadata) -> Option<(u64, u64)> {
    None
}

pub struct Monitor {
    pub home: PathBuf,
    pub warning: Option<String>,
    pub demo_mode: bool,
    limit: usize,
    files: HashMap<PathBuf, Tail>,
}

impl Monitor {
    pub fn new(home: PathBuf, limit: usize) -> Self {
        Self {
            home,
            warning: None,
            demo_mode: false,
            limit,
            files: HashMap::new(),
        }
    }
    pub fn refresh(&mut self) -> Result<()> {
        let mut candidates = Vec::new();
        let mut errors = Vec::new();
        for root in [
            self.home.join("sessions"),
            self.home.join("archived_sessions"),
        ] {
            if !root.exists() {
                continue;
            }
            for entry in WalkDir::new(root).follow_links(false) {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        errors.push(e.to_string());
                        continue;
                    }
                };
                if !entry.file_type().is_file()
                    || entry.path().extension().is_none_or(|e| e != "jsonl")
                {
                    continue;
                }
                match entry.metadata() {
                    Ok(meta) => candidates.push((
                        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        entry.into_path(),
                    )),
                    Err(e) => errors.push(e.to_string()),
                }
            }
        }
        candidates.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut keep = HashSet::new();
        let mut seen_ids = HashSet::new();
        for (_, path) in candidates {
            if keep.len() >= self.limit {
                break;
            }
            if !self.files.contains_key(&path) {
                match supported_header(&path) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(e) => {
                        errors.push(e.to_string());
                        continue;
                    }
                }
            }
            let tail = self
                .files
                .entry(path.clone())
                .or_insert_with(|| Tail::new(path.clone()));
            if let Err(e) = tail
                .read()
                .with_context(|| format!("Reading {}", path.display()))
            {
                errors.push(e.to_string());
                continue;
            }
            if tail.session.is_supported_source()
                && !tail.session.id.is_empty()
                && seen_ids.insert(tail.session.id.clone())
            {
                keep.insert(path);
            }
        }
        self.files.retain(|path, _| keep.contains(path));
        self.warning = errors.into_iter().next();
        Ok(())
    }
    pub fn sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<_> = self.files.values().map(|tail| &tail.session).collect();
        sessions.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions
    }
    pub fn models(&self, hours: u32) -> Vec<crate::models::ModelStats<'_>> {
        let since =
            (hours > 0).then(|| chrono::Utc::now() - chrono::Duration::hours(i64::from(hours)));
        crate::models::summarize(&self.sessions(), since)
    }
    pub fn snapshot(&self, hours: u32) -> Value {
        json!({
            "codex_home": self.home,
            "metric_scope": "whole_turn_including_tools_and_waiting",
            "first_output_scope": "codex_persisted_first_model_output_including_reasoning_or_tools",
            "demo": self.demo_mode,
            "warning": self.warning,
            "window_hours": hours,
            "models": self.models(hours),
            "sessions": self.sessions().into_iter().map(|s| json!({
                "id": s.id, "cwd": s.cwd, "source": s.source, "model": s.model,
                "path": s.path, "updated_at": s.updated_at, "malformed_lines": s.malformed_lines,
                "turns": s.turns.iter().map(|t| json!({
                    "id": t.id, "model": t.model, "status": t.status, "started_at": t.started_at,
                    "finished_at": t.finished_at,
                    "duration_ms": t.duration_ms, "first_output_ms": t.first_output_ms,
                    "usage": t.usage, "average_output_tps": t.average_tps(),
                    "average_visible_output_tps": t.visible_tps(),
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })
    }
    pub fn demo(&mut self) {
        self.demo_mode = true;
        for (i, project) in ["my-project", "api-server"].iter().enumerate() {
            let path = PathBuf::from(format!("demo-{i}.jsonl"));
            let mut tail = Tail::new(path.clone());
            let id = format!("demo-{i}");
            let mut feed = |kind, payload| {
                tail.session.ingest(
                    &serde_json::to_vec(&json!({
                        "timestamp":chrono::Utc::now().to_rfc3339(), "type":kind, "payload":payload
                    }))
                    .unwrap(),
                )
            };
            feed(
                "session_meta",
                json!({"id":id,"cwd":format!("/projects/{project}"),"source":"cli"}),
            );
            feed(
                "turn_context",
                json!({"model": if i == 0 {"demo-model-a"} else {"demo-model-b"}}),
            );
            feed(
                "event_msg",
                json!({"type":"task_started","turn_id":"turn-1","started_at":1790836200}),
            );
            feed(
                "token_usage_record",
                json!({"thread_id":id,"turn_id":"turn-1","response_id":"response-1",
                "turn_token_usage":{"output_tokens":702,"reasoning_output_tokens":102}}),
            );
            feed(
                "event_msg",
                json!({"type":"task_complete","turn_id":"turn-1","duration_ms":18200,"time_to_first_token_ms":2400}),
            );
            if i == 1 {
                feed(
                    "event_msg",
                    json!({"type":"task_started","turn_id":"turn-2","started_at":chrono::Utc::now().timestamp()}),
                );
            }
            self.files.insert(path, tail);
        }
    }
}

fn supported_header(path: &Path) -> Result<bool> {
    let mut line = String::new();
    BufReader::new(File::open(path)?).read_line(&mut line)?;
    // Retry incomplete headers on discovery; never classify them as malformed.
    let Ok(value) = serde_json::from_str::<Value>(&line) else {
        return Ok(false);
    };
    Ok(value["type"] == "session_meta"
        && matches!(
            value["payload"]["source"].as_str(),
            Some("cli" | "exec" | "vscode")
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    #[test]
    fn tails_partial_lines_and_recovers_from_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        fs::write(&path, b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"s\",\"source\":\"cli\"}}\n{\"type\":\"turn_context\"").unwrap();
        let mut tail = Tail::new(path.clone());
        tail.read().unwrap();
        assert_eq!(tail.session.id, "s");
        assert_eq!(tail.session.malformed_lines, 0);
        let offset = tail.offset;
        tail.read().unwrap();
        assert_eq!(tail.offset, offset);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, ",\"payload\":{{\"model\":\"test-model\"}}}}").unwrap();
        tail.read().unwrap();
        assert_eq!(tail.session.model, "test-model");
        fs::write(
            &path,
            b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"new\",\"source\":\"cli\"}}\n",
        )
        .unwrap();
        tail.read().unwrap();
        assert_eq!(tail.session.id, "new");
        assert!(tail.session.model.is_empty());
    }
    #[test]
    fn supported_sources_count_toward_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sessions");
        fs::create_dir(&root).unwrap();
        for (name, source) in [
            ("cli", "cli"),
            ("exec", "exec"),
            ("ide", "vscode"),
            ("unknown", "unknown"),
        ] {
            fs::write(root.join(format!("{name}.jsonl")),format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{name}\",\"source\":\"{source}\"}}}}\n")).unwrap();
        }
        let mut monitor = Monitor::new(dir.path().to_owned(), 3);
        monitor.refresh().unwrap();
        assert_eq!(monitor.sessions().len(), 3);
        assert!(monitor.sessions().iter().all(|s| s.is_supported_source()));
        monitor.refresh().unwrap();
        assert_eq!(monitor.sessions().len(), 3);
    }
}
