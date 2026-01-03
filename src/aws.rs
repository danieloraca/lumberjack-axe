use std::time::{Duration, SystemTime};

use aws_config::BehaviorVersion;
use aws_sdk_cloudwatchlogs::types::FilteredLogEvent;
use aws_sdk_cloudwatchlogs::{Client as CloudWatchLogsClient, Error as CloudWatchLogsError};

use thiserror::Error;

/// A single log entry returned from CloudWatch Logs.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp_millis: i64,
    pub message: String,
    pub log_stream_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum AwsLogError {
    #[error("CloudWatch Logs request failed for log group {log_group:?}: {source}")]
    CloudWatch {
        log_group: String,
        #[source]
        source: CloudWatchLogsError,
    },

    #[error("failed to list CloudWatch log groups in region {region}: {message}")]
    ListLogGroups { region: String, message: String },
}

/// High-level parameters for fetching recent logs.
pub struct FetchLogsParams<'a> {
    pub profile: Option<&'a str>,
    pub region: Option<&'a str>,
    pub log_group: &'a str,
    pub filter_pattern: Option<&'a str>,
    pub lookback: Duration,
    pub limit: i32,
}

impl<'a> Default for FetchLogsParams<'a> {
    fn default() -> Self {
        Self {
            profile: None,
            region: None,
            log_group: "",
            filter_pattern: None,
            lookback: Duration::from_secs(5 * 60),
            limit: 1_000,
        }
    }
}

async fn mk_client(profile: Option<&str>, region: Option<&str>) -> CloudWatchLogsClient {
    // Start from the new defaults-based config builder.
    let mut loader = aws_config::defaults(BehaviorVersion::latest());

    if let Some(p) = profile {
        loader = loader.profile_name(p.to_string());
    }

    if let Some(r) = region {
        let region = aws_config::Region::new(r.to_string());
        loader = loader.region(region);
    }

    let config = loader.load().await;
    CloudWatchLogsClient::new(&config)
}

/// Fetch recent log events from CloudWatch Logs using FilterLogEvents.
pub async fn fetch_recent_logs(params: FetchLogsParams<'_>) -> Result<Vec<LogEntry>, AwsLogError> {
    let client: CloudWatchLogsClient = mk_client(params.profile, params.region).await;

    let now = SystemTime::now();
    let since = now
        .checked_sub(params.lookback)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let start_time_millis = to_millis(since);

    // Build the request directly from the client.
    let mut req = client
        .filter_log_events()
        .log_group_name(params.log_group)
        .start_time(start_time_millis)
        .limit(params.limit);

    if let Some(pattern) = params.filter_pattern {
        let pattern = pattern.trim();
        if !pattern.is_empty() {
            req = req.filter_pattern(pattern);
        }
    }

    let resp = req.send().await.map_err(|e| AwsLogError::CloudWatch {
        log_group: params.log_group.to_string(),
        source: e.into(),
    })?;

    let events: Vec<LogEntry> = resp
        .events
        .unwrap_or_default()
        .into_iter()
        .map(filtered_to_entry)
        .collect();

    Ok(events)
}

fn filtered_to_entry(event: FilteredLogEvent) -> LogEntry {
    LogEntry {
        timestamp_millis: event.timestamp.unwrap_or_default(),
        message: event.message.unwrap_or_default(),
        log_stream_name: event.log_stream_name,
    }
}

fn to_millis(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => dur.as_millis().min(i64::MAX as u128) as i64,
        Err(_) => 0,
    }
}

pub async fn list_log_groups(
    profile: Option<&str>,
    region: Option<&str>,
    limit: i32,
) -> Result<Vec<String>, AwsLogError> {
    let client: CloudWatchLogsClient = mk_client(profile, region).await;

    let mut req = client.describe_log_groups();
    if limit > 0 {
        // Cap at 50 to satisfy CloudWatch constraints.
        let capped = std::cmp::min(limit, 50);
        req = req.limit(capped);
    }

    let resp = req.send().await.map_err(|e| {
        let debug_str = format!("{e:?}");
        eprintln!("DescribeLogGroups raw error: {debug_str}");

        let msg = extract_nice_aws_message_from_debug(&debug_str).unwrap_or_else(|| e.to_string());

        // Format the region nicely instead of carrying Option<String>.
        let region_display = region
            .map(|r| r.to_string())
            .unwrap_or_else(|| "<default>".to_string());

        AwsLogError::ListLogGroups {
            region: region_display,
            message: msg,
        }
    })?;

    let groups = resp
        .log_groups
        .unwrap_or_default()
        .into_iter()
        .filter_map(|g| g.log_group_name.map(|name| name.trim().to_string()))
        .collect();

    Ok(groups)
}

fn extract_nice_aws_message_from_debug(debug_str: &str) -> Option<String> {
    // Look for the JSON error payload inside the debug string.
    // The pattern looks like: b"{\"__type\":\"...\",\"message\":\"...\"}"
    if let Some(start_idx) = debug_str.find("b\"{") {
        // Find the closing quote after the JSON.
        if let Some(rest) = debug_str.get(start_idx + 2..) {
            if let Some(end_rel) = rest.find("\"}") {
                let json_slice = &rest[..end_rel + 2]; // include the closing "}
                // Unescape the Rust string-literal style quotes/backslashes.
                let unescaped = json_slice.replace("\\\"", "\"");

                // Try to parse as JSON: {"__type":"...", "message":"..."}
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&unescaped) {
                    let code = v
                        .get("__type")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    let msg = v
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !code.is_empty() || !msg.is_empty() {
                        return Some(if !code.is_empty() && !msg.is_empty() {
                            format!("{code}: {msg}")
                        } else if !code.is_empty() {
                            code
                        } else {
                            msg
                        });
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_cloudwatchlogs::types::FilteredLogEvent;

    #[test]
    fn to_millis_handles_epoch_and_positive() {
        use std::time::{Duration, SystemTime};

        assert_eq!(to_millis(SystemTime::UNIX_EPOCH), 0);

        let one_sec = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        assert_eq!(to_millis(one_sec), 1_000);

        let one_and_half = SystemTime::UNIX_EPOCH + Duration::from_millis(1500);
        assert_eq!(to_millis(one_and_half), 1_500);
    }

    #[test]
    fn filtered_to_entry_maps_fields_correctly() {
        let event = FilteredLogEvent::builder()
            .timestamp(1_700_000_000_123_i64)
            .message("hello world".to_string())
            .log_stream_name("my-stream".to_string())
            .build();

        let entry = filtered_to_entry(event);

        assert_eq!(entry.timestamp_millis, 1_700_000_000_123_i64);
        assert_eq!(entry.message, "hello world");
        assert_eq!(entry.log_stream_name.as_deref(), Some("my-stream"));
    }

    #[test]
    fn filtered_to_entry_handles_missing_fields() {
        let event = FilteredLogEvent::builder().build();

        let entry = filtered_to_entry(event);

        // Defaults when fields are missing
        assert_eq!(entry.timestamp_millis, 0);
        assert_eq!(entry.message, "");
        assert_eq!(entry.log_stream_name, None);
    }

    #[test]
    fn fetch_logs_params_default_values() {
        let params = FetchLogsParams::default();
        assert_eq!(params.region, None);
        assert_eq!(params.log_group, "");
        assert_eq!(params.filter_pattern, None);
        assert_eq!(params.lookback, Duration::from_secs(5 * 60));
        assert_eq!(params.limit, 1_000);
    }
}
