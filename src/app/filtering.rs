use std::time::Duration;

/// What kind of time range is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRangeKind {
    Last5m,
    Last15m,
    Last1h,
    Custom, // last X seconds or minutes
}

/// Configuration for time range selection.
#[derive(Debug, Clone)]
pub struct TimeRangeConfig {
    pub kind: TimeRangeKind,
    /// Custom value for `Custom` kind (e.g. 30 seconds or 10 minutes).
    pub custom_value: u64,
    /// Whether custom_value is interpreted as seconds or minutes.
    pub custom_is_minutes: bool,
}

impl Default for TimeRangeConfig {
    fn default() -> Self {
        Self {
            kind: TimeRangeKind::Last5m,
            custom_value: 5,
            custom_is_minutes: true, // "last 5 minutes" by default
        }
    }
}

impl TimeRangeConfig {
    /// Compute a lookback `Duration` based on the current config.
    pub fn lookback_duration(&self) -> Duration {
        match self.kind {
            TimeRangeKind::Last5m => Duration::from_secs(5 * 60),
            TimeRangeKind::Last15m => Duration::from_secs(15 * 60),
            TimeRangeKind::Last1h => Duration::from_secs(60 * 60),
            TimeRangeKind::Custom => {
                let secs = if self.custom_is_minutes {
                    self.custom_value.saturating_mul(60)
                } else {
                    self.custom_value
                };
                Duration::from_secs(secs.max(1))
            }
        }
    }
}

/// Combined filter configuration (text + time range).
#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub filter_text: String,
    pub time_range: TimeRangeConfig,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            filter_text: String::new(),
            time_range: TimeRangeConfig::default(),
        }
    }
}
