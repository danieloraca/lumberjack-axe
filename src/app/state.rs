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
