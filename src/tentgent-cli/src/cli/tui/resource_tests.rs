use super::*;
use serde_json::json;
use std::fs;

#[test]
fn disk_free_thresholds_classify_low_unknown_and_healthy() {
    assert_eq!(classify_disk(None, Some(1)), DiskState::Unknown);
    assert_eq!(
        classify_disk(Some(100 * 1024 * 1024 * 1024), Some(4 * 1024 * 1024 * 1024)),
        DiskState::Low
    );
    assert_eq!(
        classify_disk(
            Some(100 * 1024 * 1024 * 1024),
            Some(50 * 1024 * 1024 * 1024)
        ),
        DiskState::Healthy
    );
}

#[test]
fn parse_ps_handles_rss_cpu_and_unverified_identity() {
    let parsed =
        parse_ps_output("123 4096 1.5 /usr/bin/python\n", Some("tentgent")).expect("process");

    match parsed {
        ProcessProbe::Found {
            rss_kib,
            cpu_percent,
            identity,
            command,
        } => {
            assert_eq!(rss_kib, Some(4096));
            assert_eq!(cpu_percent, Some(1.5));
            assert_eq!(identity, ProcessIdentity::ExistsUnverified);
            assert!(command.contains("python"));
        }
        _ => panic!("expected found process"),
    }
}

#[test]
fn parse_df_classifies_low_disk() {
    let output = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk1 10000000 9600000 400000 96% /\n";
    let disk = parse_df_output(Path::new("/tmp"), output);

    assert_eq!(disk.state, DiskState::Low);
    assert_eq!(disk.available_bytes, Some(400000 * 1024));
    assert_eq!(disk.used_percent, Some(96.0));
}

#[test]
fn directory_scan_counts_bytes_and_skips_symlink_targets() {
    let home = unique_home("scan");
    fs::create_dir_all(home.join("models")).expect("models dir");
    fs::write(home.join("models/model.bin"), vec![1_u8; 128]).expect("model file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(home.join("models"), home.join("models-link")).expect("symlink");

    let inspection = test_inspection(&home);
    let mut budget = ScanBudget::new(25, Duration::from_secs(1));
    let row = scan_storage_row(
        StorageCategory {
            label: "models",
            path: home.join("models"),
        },
        &mut budget,
    );

    assert!(row.exists);
    assert_eq!(row.total_bytes, 128);
    assert_eq!(row.file_count, 1);
    assert!(!row.partial);
    assert!(storage_categories(&home, &inspection)
        .iter()
        .any(|category| category.label == "logs"));
}

#[test]
fn directory_scan_marks_partial_on_budget() {
    let home = unique_home("partial");
    fs::create_dir_all(&home).expect("home");
    for index in 0..5 {
        fs::write(home.join(format!("{index}.txt")), "x").expect("file");
    }
    let mut budget = ScanBudget::new(2, Duration::from_secs(1));

    let row = scan_storage_row(
        StorageCategory {
            label: "tiny",
            path: home,
        },
        &mut budget,
    );

    assert!(row.partial);
}

#[test]
fn warnings_cover_large_log_stale_pid_and_train_age() {
    let storage = vec![StorageRow {
        category: "logs".to_string(),
        path: PathBuf::from("/tmp/logs"),
        exists: true,
        total_bytes: LARGE_LOG_BYTES + 1,
        file_count: 1,
        scanned_files: 1,
        skipped_unreadable: 0,
        partial: false,
        largest_file: Some(ResourceFileSummary {
            path: PathBuf::from("/tmp/logs/daemon.stderr.log"),
            bytes: LARGE_LOG_BYTES + 1,
        }),
    }];
    let process = vec![ProcessRow {
        source: "daemon".to_string(),
        ref_label: "daemon".to_string(),
        pid: Some(999_999),
        state: "running".to_string(),
        rss_kib: None,
        cpu_percent: None,
        identity: ProcessIdentity::Missing,
        port_or_source: "-".to_string(),
        detail: "missing".to_string(),
    }];
    let train = NavigatorRow {
        item_ref: "run-1".to_string(),
        short_ref: "run-1".to_string(),
        columns: Vec::new(),
        search_text: String::new(),
        summary: Vec::new(),
        raw: json!({
            "run_ref": "run-1",
            "status": "running",
            "stale": true
        }),
    };
    let disk = DiskSummary {
        path: PathBuf::from("/tmp"),
        total_bytes: Some(100),
        available_bytes: Some(50),
        used_percent: Some(50.0),
        state: DiskState::Healthy,
        detail: "ok".to_string(),
    };

    let warnings = build_warnings(&storage, &process, &disk, &[train]);

    assert!(warnings
        .iter()
        .any(|warning| warning.message.contains("large log")));
    assert!(warnings
        .iter()
        .any(|warning| warning.message.contains("process is missing")));
    assert!(warnings
        .iter()
        .any(|warning| warning.message.contains("marked stale")));
}

#[test]
fn train_age_warning_requires_timestamp_and_threshold() {
    let running_without_timestamp = NavigatorRow {
        item_ref: "run-unknown".to_string(),
        short_ref: "run-unknown".to_string(),
        columns: Vec::new(),
        search_text: String::new(),
        summary: Vec::new(),
        raw: json!({
            "run_ref": "run-unknown",
            "status": "running"
        }),
    };
    assert!(train_run_age_warning(&running_without_timestamp).is_none());

    let old_started_at = (OffsetDateTime::now_utc() - time::Duration::hours(7))
        .format(&Rfc3339)
        .expect("format timestamp");
    let old_running = NavigatorRow {
        item_ref: "run-old".to_string(),
        short_ref: "run-old".to_string(),
        columns: Vec::new(),
        search_text: String::new(),
        summary: Vec::new(),
        raw: json!({
            "run_ref": "run-old",
            "status": "running",
            "started_at": old_started_at
        }),
    };

    let warning = train_run_age_warning(&old_running).expect("old run warning");
    assert!(warning.message.contains("more than 6h"));
}

fn test_inspection(home: &Path) -> DaemonInspection {
    DaemonInspection {
        home_dir: home.to_path_buf(),
        runtime_dir: home.join("runtime"),
        log_dir: home.join("logs"),
        process_path: home.join("runtime/daemon.toml"),
        pid_path: home.join("runtime/tentgent.pid"),
        stdout_log_path: home.join("logs/daemon.stdout.log"),
        stderr_log_path: home.join("logs/daemon.stderr.log"),
        running: false,
        process: None,
    }
}

fn unique_home(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    env::temp_dir().join(format!("tentgent-resource-{label}-{nanos}"))
}
