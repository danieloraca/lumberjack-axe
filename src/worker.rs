use std::time::Duration;

use std::sync::mpsc::{Receiver, Sender};

use crate::aws::{AwsLogError, FetchLogsParams, LogEntry};

pub enum WorkerRequest {
    /// Fetch recent logs for given params, sending result on the provided channel.
    FetchRecentLogs {
        profile: Option<String>,
        region: Option<String>,
        log_group: String,
        filter_pattern: Option<String>,
        lookback: Duration,
        limit: i32,
        respond_to: Sender<Result<Vec<LogEntry>, AwsLogError>>,
    },

    /// List log groups for given profile/region, sending result on the provided channel.
    ListLogGroups {
        profile: Option<String>,
        region: Option<String>,
        limit: i32,
        respond_to: Sender<Result<Vec<String>, AwsLogError>>,
    },
}

/// Handle for sending work to the worker.
#[derive(Clone)]
pub struct WorkerHandle {
    sender: Sender<WorkerRequest>,
}

impl WorkerHandle {
    pub fn send(&self, req: WorkerRequest) {
        // Best-effort send; if worker is gone, we just ignore.
        let _ = self.sender.send(req);
    }
}

/// Spawn the worker thread and return a handle for sending it requests.
///
/// The worker runs a single-threaded Tokio runtime (current_thread), mirroring aws_test.
pub fn spawn_worker() -> WorkerHandle {
    use std::thread;

    let (tx, rx): (Sender<WorkerRequest>, Receiver<WorkerRequest>) = std::sync::mpsc::channel();

    thread::spawn(move || {
        // Build a current_thread runtime, like #[tokio::main(flavor = "current_thread")].
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime for worker");

        rt.block_on(async move {
            worker_loop(rx).await;
        });
    });

    WorkerHandle { sender: tx }
}

async fn worker_loop(rx: Receiver<WorkerRequest>) {
    use crate::aws::{fetch_recent_logs, list_log_groups};

    while let Ok(req) = rx.recv() {
        match req {
            WorkerRequest::FetchRecentLogs {
                profile,
                region,
                log_group,
                filter_pattern,
                lookback,
                limit,
                respond_to,
            } => {
                let params = FetchLogsParams {
                    profile: profile.as_deref(),
                    region: region.as_deref(),
                    log_group: &log_group,
                    filter_pattern: filter_pattern.as_deref(),
                    lookback,
                    limit,
                };
                let result = fetch_recent_logs(params).await;
                let _ = respond_to.send(result);
            }
            WorkerRequest::ListLogGroups {
                profile,
                region,
                limit,
                respond_to,
            } => {
                let profile_opt = profile.as_deref();
                let region_opt = region.as_deref();
                let result = list_log_groups(profile_opt, region_opt, limit).await;
                let _ = respond_to.send(result);
            }
        }
    }
}
