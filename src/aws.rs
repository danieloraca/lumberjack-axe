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

    #[error("failed to list CloudWatch log groups in region {region:?}: {message}")]
    ListLogGroups {
        region: Option<String>,
        message: String,
    },
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
    let mut loader = aws_config::from_env();

    if let Some(p) = profile {
        loader = loader.profile_name(p.to_string());
    }

    if let Some(r) = region {
        let region = aws_config::Region::new(r.to_string());
        loader = loader.region(region);
    } else {
        // No explicit region: we *don't* special-case here; we just let
        // the default env/config chain decide, like aws_test does.
        // (We still keep BehaviorVersion for future-proof defaults.)
        return CloudWatchLogsClient::new(
            &aws_config::load_defaults(BehaviorVersion::latest()).await,
        );
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
        eprintln!("DescribeLogGroups raw error: {e:?}");
        AwsLogError::ListLogGroups {
            region: region.map(str::to_string),
            message: e.to_string(),
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
