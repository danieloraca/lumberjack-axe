use chrono::{Local, LocalResult, TimeZone, Utc};
use serde_json::Value as JsonValue;

use crate::aws::LogEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Logs,
    // Settings,
    // Favorites,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    RetroGreen,
}

#[derive(Default)]
pub struct LogsViewState {
    pub profile: String,
    pub region: String,
    pub log_group: String,
    pub filter_text: String,
    pub available_groups: Vec<String>,
    pub selected_group_index: Option<usize>,
    pub tail_mode: bool,
    pub show_local_time: bool,
    pub entries: Vec<LogEntry>,
    pub tail_interval_secs: u64,
    pub last_tail_instant: Option<std::time::Instant>,
}

impl LogsViewState {
    pub fn new_default() -> Self {
        Self {
            profile: "form".to_string(),
            region: "eu-west-1".to_string(),
            log_group: String::new(),
            filter_text: String::new(),
            tail_mode: false,
            show_local_time: false,
            entries: Vec::new(),
            available_groups: Vec::new(),
            selected_group_index: None,
            tail_interval_secs: 5,
            last_tail_instant: None,
        }
    }
}

pub fn format_timestamp_millis(ts_millis: i64, use_local: bool) -> String {
    if ts_millis <= 0 {
        return "-".to_string();
    }

    let secs = ts_millis / 1000;
    let nanos = (ts_millis % 1000) * 1_000_000;

    if use_local {
        match Local.timestamp_opt(secs, nanos as u32) {
            LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            _ => "-".to_string(),
        }
    } else {
        match Utc.timestamp_opt(secs, nanos as u32) {
            LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S%.3fZ").to_string(),
            _ => "-".to_string(),
        }
    }
}

pub fn try_pretty_json(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let first = bytes[0] as char;
    let last = bytes[bytes.len() - 1] as char;
    if !((first == '{' && last == '}') || (first == '[' && last == ']')) {
        return None;
    }

    match serde_json::from_str::<JsonValue>(trimmed) {
        Ok(v) => serde_json::to_string_pretty(&v).ok(),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_view_state_defaults_are_sensible() {
        let s = LogsViewState::new_default();

        assert_eq!(s.profile, "form");
        assert_eq!(s.region, "eu-west-1");
        assert_eq!(s.log_group, "");
        assert_eq!(s.filter_text, "");
        assert!(!s.tail_mode);
        assert!(!s.show_local_time);
        assert!(s.entries.is_empty());
        assert!(s.available_groups.is_empty());
        assert_eq!(s.selected_group_index, None);
        assert_eq!(s.tail_interval_secs, 5);
        assert!(s.last_tail_instant.is_none());
    }

    #[test]
    fn format_timestamp_millis_handles_zero_and_positive() {
        let utc = format_timestamp_millis(0, false);
        assert_eq!(utc, "-");

        let ts = 1_700_000_000_123_i64; // just some millis
        let utc = format_timestamp_millis(ts, false);
        assert!(utc.ends_with('Z')); // UTC has Z suffix

        let local = format_timestamp_millis(ts, true);
        assert!(!local.ends_with('Z')); // local doesn't
    }

    #[test]
    fn try_pretty_json_prettifies_valid_json_and_rejects_non_json() {
        let raw = r#"{"a":1,"b":{"c":2}}"#;
        let pretty = try_pretty_json(raw).expect("should parse");
        assert!(pretty.contains("\n")); // multi-line
        assert!(pretty.contains("\"a\""));
        assert!(pretty.contains("\"b\""));

        assert_eq!(try_pretty_json("not json"), None);
        assert_eq!(try_pretty_json("{not json}"), None);
    }
}
