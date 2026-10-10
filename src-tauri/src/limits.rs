//! Official usage limits.
//! - Claude: `~/.claude/.credentials.json` OAuth token → `api.anthropic.com/api/oauth/usage`.
//!   Claude Code refreshes that token itself; an expired one means "run Claude Code once".
//! - Codex: the latest `rate_limits` snapshot Codex already writes into its rollout logs
//!   (`token_count` events), so no `codex app-server` process is needed.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Newest rollout files scanned for a Codex snapshot.
const CODEX_FILES_TO_SCAN: usize = 5;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Window {
    /// "5h", "7d", "7d-opus" … ; the UI localizes.
    pub key: String,
    pub used_percent: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderLimits {
    pub provider: &'static str,
    pub windows: Vec<Window>,
    /// Shown instead of bars when nothing could be read.
    pub error: Option<String>,
}

impl ProviderLimits {
    fn from(provider: &'static str, r: Result<Vec<Window>, String>) -> Self {
        match r {
            Ok(windows) => ProviderLimits { provider, windows, error: None },
            Err(e) => ProviderLimits { provider, windows: vec![], error: Some(e) },
        }
    }
}

pub fn read_all() -> Vec<ProviderLimits> {
    let home = std::env::home_dir().unwrap_or_default();
    vec![
        ProviderLimits::from("claude", claude(&home)),
        ProviderLimits::from("codex", codex(&home.join(".codex/sessions"))),
    ]
}

/// Highest window utilization, for the tray label.
pub fn peak(all: &[ProviderLimits]) -> Option<f64> {
    all.iter().flat_map(|p| &p.windows).map(|w| w.used_percent).reduce(f64::max)
}

fn claude(home: &Path) -> Result<Vec<Window>, String> {
    let creds: Value = fs::read(home.join(".claude/.credentials.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("no_credentials")?;
    let oauth = &creds["claudeAiOauth"];
    let token = oauth["accessToken"].as_str().ok_or("no_credentials")?;
    if let Some(ms) = oauth["expiresAt"].as_i64() {
        if ms < Utc::now().timestamp_millis() {
            return Err("token_expired".into());
        }
    }
    let resp = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build()
        .get(CLAUDE_USAGE_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .call();
    let body: Value = match resp {
        Ok(r) => r.into_json().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(401 | 403, _)) => return Err("token_expired".into()),
        Err(ureq::Error::Status(429, _)) => return Err("rate_limited".into()),
        Err(e) => return Err(e.to_string()),
    };
    Ok(parse_claude(&body))
}

pub fn parse_claude(body: &Value) -> Vec<Window> {
    [("five_hour", "5h"), ("seven_day", "7d"), ("seven_day_opus", "7d-opus"), ("seven_day_sonnet", "7d-sonnet")]
        .iter()
        .filter_map(|(field, key)| {
            let w = body.get(*field)?;
            Some(Window {
                key: key.to_string(),
                used_percent: w.get("utilization")?.as_f64()?,
                resets_at: w["resets_at"].as_str().map(str::to_string),
            })
        })
        .collect()
}

fn newest_rollouts(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            match e.metadata() {
                Ok(m) if m.is_dir() => walk(&p, out),
                Ok(m) if p.extension().is_some_and(|x| x == "jsonl") => {
                    out.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), p))
                }
                _ => {}
            }
        }
    }
    let mut files = vec![];
    walk(root, &mut files);
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(CODEX_FILES_TO_SCAN).map(|(_, p)| p).collect()
}

fn codex(root: &Path) -> Result<Vec<Window>, String> {
    for file in newest_rollouts(root) {
        let Ok(text) = fs::read_to_string(&file) else { continue };
        let latest = text
            .lines()
            .rev()
            .filter(|l| l.contains("\"rate_limits\""))
            .find_map(|l| parse_codex_line(l, Utc::now()).filter(|w| !w.is_empty()));
        if let Some(windows) = latest {
            return Ok(windows);
        }
    }
    Err("no_data".into())
}

/// A window whose reset time has passed is shown as 0%: the snapshot predates the reset.
pub fn parse_codex_line(line: &str, now: DateTime<Utc>) -> Option<Vec<Window>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let limits = &v["payload"]["rate_limits"];
    let windows = ["primary", "secondary"]
        .iter()
        .filter_map(|slot| {
            let w = limits.get(*slot).filter(|w| w.is_object())?;
            let minutes = w["window_minutes"].as_u64()?;
            let resets = w["resets_at"].as_i64().and_then(|s| DateTime::from_timestamp(s, 0));
            let expired = resets.is_some_and(|r| r <= now);
            Some(Window {
                key: match minutes {
                    m if m % 1440 == 0 => format!("{}d", m / 1440),
                    m if m % 60 == 0 => format!("{}h", m / 60),
                    m => format!("{m}m"),
                },
                used_percent: if expired { 0.0 } else { w["used_percent"].as_f64()? },
                resets_at: resets.filter(|_| !expired).map(|r| r.to_rfc3339()),
            })
        })
        .collect();
    Some(windows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_skips_null_windows() {
        let body = json!({
            "five_hour": { "utilization": 7.0, "resets_at": "2026-10-10T11:10:00+00:00" },
            "seven_day": { "utilization": 42.0, "resets_at": null },
            "seven_day_opus": null
        });
        let w = parse_claude(&body);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].key.as_str(), w[0].used_percent), ("5h", 7.0));
        assert_eq!(w[1].resets_at, None);
    }

    #[test]
    fn codex_parses_and_zeroes_expired_windows() {
        let line = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{
            "primary":{"used_percent":30.0,"window_minutes":300,"resets_at":2000},
            "secondary":{"used_percent":12.0,"window_minutes":10080,"resets_at":1000}}}}"#
            .replace('\n', "");
        let now = DateTime::from_timestamp(1500, 0).unwrap();
        let w = parse_codex_line(&line, now).unwrap();
        assert_eq!((w[0].key.as_str(), w[0].used_percent), ("5h", 30.0));
        assert_eq!((w[1].key.as_str(), w[1].used_percent, w[1].resets_at.clone()), ("7d", 0.0, None));

        let empty = r#"{"payload":{"rate_limits":{"primary":null,"secondary":null}}}"#;
        assert_eq!(parse_codex_line(empty, now), Some(vec![]));
    }

    /// `cargo test real_limits -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_limits() {
        println!("{:#?}", read_all());
    }
}
