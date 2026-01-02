use aws_sdk_cloudwatchlogs::Client as CloudWatchLogsClient;
use aws_sdk_cloudwatchlogs::types::FilteredLogEvent;
use std::env;
use std::time::{Duration, SystemTime};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("=== aws_test ===");

    let profile = env::var("PROFILE").ok();
    let region = env::var("REGION").ok();
    let log_group = match env::var("LOG_GROUP") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            eprintln!("LOG_GROUP env var is required");
            std::process::exit(1);
        }
    };

    println!("Profile: {:?}", profile.as_deref().unwrap_or("<default>"));
    println!("Region:  {:?}", region.as_deref().unwrap_or("<auto>"));
    println!("LogGroup: {:?}", log_group);

    let mut loader = aws_config::from_env();

    if let Some(ref p) = profile {
        println!("Using profile {:?}", p);
        loader = loader.profile_name(p.clone());
    }

    if let Some(ref r) = region {
        println!("Using region {:?}", r);
        let region = aws_config::Region::new(r.clone());
        loader = loader.region(region);
    } else {
        println!("No explicit region set; will rely on defaults (may fail).");
    }

    println!("Loading AWS config...");
    let config = loader.load().await;

    let client = CloudWatchLogsClient::new(&config);

    // 1) Try DescribeLogGroups to see if we can list anything.
    println!("\n=== DescribeLogGroups (first page) ===");
    match client.describe_log_groups().limit(20).send().await {
        Ok(resp) => {
            let groups: Vec<_> = resp
                .log_groups
                .unwrap_or_default()
                .into_iter()
                .filter_map(|g| g.log_group_name)
                .collect();
            println!("Got {} log groups:", groups.len());
            for g in groups {
                println!("  - {g}");
            }
        }
        Err(e) => {
            eprintln!("DescribeLogGroups ERROR (Debug): {e:?}");
            eprintln!("DescribeLogGroups ERROR (Display): {e}");
        }
    }

    // 2) Try FilterLogEvents on the specific LOG_GROUP.
    println!("\n=== FilterLogEvents (last 5 minutes) ===");
    let now = SystemTime::now();
    let since = now
        .checked_sub(Duration::from_secs(5 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let start_time_millis = to_millis(since);

    let req = client
        .filter_log_events()
        .log_group_name(&log_group)
        .start_time(start_time_millis)
        .limit(50);

    match req.send().await {
        Ok(resp) => {
            let events: Vec<FilteredLogEvent> = resp.events.unwrap_or_default();
            println!("Got {} events:", events.len());
            for e in events {
                let ts = e.timestamp.unwrap_or_default();
                let msg = e.message.unwrap_or_default();
                let stream = e.log_stream_name.unwrap_or_default();
                println!("[{ts}] ({stream}) {msg}");
            }
        }
        Err(e) => {
            eprintln!("FilterLogEvents ERROR (Debug): {e:?}");
            eprintln!("FilterLogEvents ERROR (Display): {e}");
        }
    }

    println!("\n=== aws_test done ===");
}

fn to_millis(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => dur.as_millis().min(i64::MAX as u128) as i64,
        Err(_) => 0,
    }
}
