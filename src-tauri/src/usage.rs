//! Today's token usage read straight from local CLI logs.
//! - Claude: `~/.claude/projects/**/*.jsonl`, `type:"assistant"` lines with `message.usage`.
//!   Resumed sessions/sidechains log the same turn again, so dedup by `(message.id, requestId)`
//!   and keep the largest total (streaming logs partial output first).
//! - Codex: `~/.codex/sessions/**/rollout-*.jsonl`, `payload.type:"token_count"` → `info.last_token_usage`.

use chrono::{DateTime, Local, NaiveDate};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
    fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_write += o.cache_write;
        self.cache_read += o.cache_read;
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct TodayUsage {
    pub claude: Tokens,
    pub codex: Tokens,
    pub total: u64,
}

fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn local_day(ts: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
}

/// Returns `(dedup key, tokens)` for a Claude assistant line logged on `day`.
pub fn parse_claude_line(line: &str, day: NaiveDate) -> Option<(String, Tokens)> {
    let obj: Value = serde_json::from_str(line).ok()?;
    if obj.get("type")?.as_str()? != "assistant" {
        return None;
    }
    if local_day(obj.get("timestamp")?.as_str()?)? != day {
        return None;
    }
    let msg = obj.get("message")?;
    let usage = msg.get("usage")?;
    let id = format!(
        "{}|{}",
        msg.get("id").and_then(Value::as_str).unwrap_or(""),
        obj.get("requestId").and_then(Value::as_str).unwrap_or("")
    );
    Some((
        id,
        Tokens {
            input: n(usage, "input_tokens"),
            output: n(usage, "output_tokens"),
            cache_write: n(usage, "cache_creation_input_tokens"),
            cache_read: n(usage, "cache_read_input_tokens"),
        },
    ))
}

/// Codex `last_token_usage` is the turn delta. input_tokens includes cached input.
pub fn parse_codex_line(line: &str, day: NaiveDate) -> Option<Tokens> {
    if !line.contains("token_count") {
        return None;
    }
    let obj: Value = serde_json::from_str(line).ok()?;
    let payload = obj.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    if local_day(obj.get("timestamp")?.as_str()?)? != day {
        return None;
    }
    let last = payload.get("info")?.get("last_token_usage")?;
    let cached = n(last, "cached_input_tokens");
    Some(Tokens {
        input: n(last, "input_tokens").saturating_sub(cached),
        output: n(last, "output_tokens"),
        cache_write: 0,
        cache_read: cached,
    })
}

/// `*.jsonl` under `root`, modified on/after `since`.
fn jsonl_files(root: &Path, since: SystemTime, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            jsonl_files(&path, since, out);
        } else if path.extension().is_some_and(|x| x == "jsonl")
            && meta.modified().is_ok_and(|m| m >= since)
        {
            out.push(path);
        }
    }
}

fn lines(path: &Path) -> impl Iterator<Item = String> {
    fs::File::open(path)
        .ok()
        .into_iter()
        .flat_map(|f| BufReader::new(f).lines().map_while(Result::ok))
}

fn start_of_today() -> SystemTime {
    let midnight = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    midnight
        .and_local_timezone(Local)
        .earliest()
        .map(SystemTime::from)
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

// ponytail: full rescan each refresh. Add per-file (mtime, offset) cache when logs get large.
pub fn read_today() -> TodayUsage {
    let Some(home) = std::env::home_dir() else {
        return TodayUsage::default();
    };
    let day = Local::now().date_naive();
    let since = start_of_today();

    let mut files = Vec::new();
    jsonl_files(&home.join(".claude/projects"), since, &mut files);
    let mut by_id: HashMap<String, Tokens> = HashMap::new();
    for f in &files {
        for line in lines(f) {
            if let Some((id, t)) = parse_claude_line(&line, day) {
                let e = by_id.entry(id).or_default();
                if t.total() > e.total() {
                    *e = t;
                }
            }
        }
    }
    let mut claude = Tokens::default();
    by_id.values().for_each(|t| claude.add(t));

    // ponytail: sums every token_count line. Forked/subagent rollouts replay parent turns
    // (original handles that in resolveCodexRollouts); port it if Codex totals look inflated.
    let mut files = Vec::new();
    jsonl_files(&home.join(".codex/sessions"), since, &mut files);
    let mut codex = Tokens::default();
    for f in &files {
        for line in lines(f) {
            if let Some(t) = parse_codex_line(&line, day) {
                codex.add(&t);
            }
        }
    }

    TodayUsage {
        total: claude.total() + codex.total(),
        claude,
        codex,
    }
}

/// `200.7M` style compact number, like the original menu bar label.
pub fn compact(v: u64) -> String {
    let f = v as f64;
    match v {
        0..1_000 => v.to_string(),
        1_000..1_000_000 => format!("{:.1}K", f / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", f / 1e6),
        _ => format!("{:.1}B", f / 1e9),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today_ts() -> String {
        Local::now().to_rfc3339()
    }

    #[test]
    fn claude_line_parses_and_filters() {
        let day = Local::now().date_naive();
        let line = format!(
            r#"{{"type":"assistant","requestId":"r1","timestamp":"{}","message":{{"id":"m1","usage":{{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}}}}}}"#,
            today_ts()
        );
        let (id, t) = parse_claude_line(&line, day).unwrap();
        assert_eq!(id, "m1|r1");
        assert_eq!(t.total(), 10);
        assert!(parse_claude_line(&line.replace("assistant", "user"), day).is_none());
        assert!(parse_claude_line(&line, day.succ_opt().unwrap()).is_none());
    }

    #[test]
    fn codex_line_splits_cached_input() {
        let day = Local::now().date_naive();
        let line = format!(
            r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":100,"cached_input_tokens":60,"output_tokens":5}}}}}}}}"#,
            today_ts()
        );
        let t = parse_codex_line(&line, day).unwrap();
        assert_eq!(
            t,
            Tokens {
                input: 40,
                output: 5,
                cache_write: 0,
                cache_read: 60
            }
        );
    }

    #[test]
    fn compact_formats() {
        assert_eq!(compact(999), "999");
        assert_eq!(compact(200_700_000), "200.7M");
    }
}

/// `cargo test real_logs -- --ignored --nocapture` prints today's usage from this machine.
#[cfg(test)]
#[test]
#[ignore]
fn real_logs() {
    println!("{:?}", read_today());
}
