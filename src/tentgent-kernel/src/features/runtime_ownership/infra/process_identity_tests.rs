use crate::foundation::error::KernelError;

use super::process_identity::{
    parse_tasklist_output, parse_unix_process_state, unix_liveness_from_state,
};

#[cfg(unix)]
#[test]
fn unix_invalid_single_process_ids_are_dead_without_group_probes() {
    use crate::features::runtime_ownership::OwnershipProcessProbe;

    for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
        assert!(!super::StdOwnershipProcessProbe
            .is_process_running(pid)
            .expect("invalid single-process PID must not invoke kill or ps"));
    }
}

#[cfg(unix)]
#[test]
fn unix_ownership_probe_detects_current_process() {
    use crate::features::runtime_ownership::OwnershipProcessProbe;

    assert!(super::StdOwnershipProcessProbe
        .is_process_running(std::process::id())
        .expect("current process must be live"));
}

#[test]
fn unix_state_probe_recognizes_zombies_without_changing_live_states() {
    for state in ["Z", "Z+", "ZN", " Zs\n"] {
        assert!(parse_unix_process_state(true, state, "").unwrap());
    }
    for state in ["R+", "Ss", "D", "T", "t", "I", "U", "W", "X"] {
        assert!(!parse_unix_process_state(true, state, "").unwrap());
    }
}

#[test]
fn unix_state_probe_failure_is_unknown_even_with_zombie_stdout() {
    for stderr in ["", "Access denied", "拒絕存取"] {
        let error = parse_unix_process_state(false, "Z-private-output", stderr)
            .expect_err("failed ps must not prove that a process is dead");
        assert!(matches!(error, KernelError::RuntimeOwnershipUnavailable(_)));
        assert!(!error.to_string().contains("private-output"));
    }
}

#[test]
fn unix_state_probe_empty_or_ambiguous_success_is_unknown() {
    for stdout in ["", "  \n", "123", "S Z", "Z\nR"] {
        assert!(parse_unix_process_state(true, stdout, "").is_err());
    }
}

#[test]
fn unix_state_probe_failure_limits_stderr_details() {
    let stderr = format!("{}do-not-include", "拒".repeat(256));
    let error = parse_unix_process_state(false, "", &stderr).unwrap_err();
    assert!(error.to_string().contains(&"拒".repeat(256)));
    assert!(!error.to_string().contains("do-not-include"));
}

#[test]
fn unix_failed_state_probe_only_accepts_independently_verified_exit() {
    let unknown_state = || parse_unix_process_state(false, "", "ps failed");
    assert!(!unix_liveness_from_state(unknown_state(), || Ok(false)).unwrap());
    let live = unix_liveness_from_state(unknown_state(), || Ok(true)).unwrap_err();
    assert!(live.to_string().contains("ps failed"));
    let unknown = unix_liveness_from_state(unknown_state(), || {
        Err(KernelError::RuntimeOwnershipUnavailable(
            "signal probe failed".to_string(),
        ))
    })
    .unwrap_err();
    assert!(unknown.to_string().contains("signal probe failed"));
}

#[test]
fn unix_successful_state_probe_needs_no_second_signal_probe() {
    for zombie in [false, true] {
        assert_eq!(
            unix_liveness_from_state(Ok(zombie), || panic!("unexpected second probe")).unwrap(),
            !zombie
        );
    }
}

#[test]
fn tasklist_matches_only_the_exact_pid_field() {
    let rows = "\"worker123.exe\",\"1234\",\"Console\",\"1\",\"123 K\"\r\n\"worker.exe\",\"5123\",\"Console\",\"1\",\"1 K\"\r\n";
    assert!(!parse_tasklist_output(123, true, rows, "").unwrap());
    assert!(parse_tasklist_output(1234, true, rows, "").unwrap());
    assert!(parse_tasklist_output(5123, true, rows, "").unwrap());
}

#[test]
fn tasklist_full_table_matches_and_absent_pids_do_not_require_localized_messages() {
    let rows = concat!(
        "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n",
        "\"中文程序.exe\",\"512\",\"Console\",\"1\",\"1,024 K\"\r\n",
        "\"tentgent.exe\",\"4096\",\"Console\",\"1\",\"2,048 K\"\r\n",
    );
    for pid in [0, 512, 4096] {
        assert!(parse_tasklist_output(pid, true, rows, "").unwrap());
    }
    for pid in [12, 51, 513, 40960] {
        assert!(!parse_tasklist_output(pid, true, rows, "").unwrap());
    }
}

#[test]
fn tasklist_supports_csv_quoting_in_image_names() {
    let rows = "\"worker,\"\"quoted\"\".exe\",\"123\",\"Console\",\"1\",\"4,096 K\"";
    assert!(parse_tasklist_output(123, true, rows, "").unwrap());
}

#[test]
fn tasklist_empty_and_info_results_mean_no_process() {
    for output in [
        "",
        "\r\n  \n",
        "INFO: No tasks are running which match the specified criteria.\r\n",
    ] {
        assert!(!parse_tasklist_output(123, true, output, "").unwrap());
    }
}

#[test]
fn tasklist_failed_exit_is_unknown_even_with_matching_stdout() {
    let error = parse_tasklist_output(123, false, "\"worker.exe\",\"123\"", "Access denied")
        .expect_err("failed probe must not report a stopped process");
    assert!(matches!(error, KernelError::RuntimeOwnershipUnavailable(_)));
    assert!(error.to_string().contains("Access denied"));
}

#[test]
fn tasklist_failed_exit_never_exposes_the_process_table() {
    let error = parse_tasklist_output(123, false, "\"private-program.exe\",\"123\"", "")
        .expect_err("failed command remains unknown");
    assert!(error.to_string().contains("command exited unsuccessfully"));
    assert!(!error.to_string().contains("private-program.exe"));
}

#[test]
fn tasklist_failed_exit_limits_stderr_details() {
    let stderr = format!("{}do-not-include", "拒".repeat(256));
    let error = parse_tasklist_output(123, false, "", &stderr).unwrap_err();
    assert!(error.to_string().contains(&"拒".repeat(256)));
    assert!(!error.to_string().contains("do-not-include"));
}

#[test]
fn tasklist_unexpected_localized_prose_is_unknown_not_stopped() {
    let error = parse_tasklist_output(123, true, "資訊: 沒有符合指定準則的工作。", "")
        .expect_err("production expects an unfiltered CSV table");
    assert!(matches!(error, KernelError::RuntimeOwnershipUnavailable(_)));
}

#[test]
fn tasklist_malformed_success_is_unknown_not_stopped() {
    for output in [
        "unexpected output",
        "\"worker.exe\",\"not-a-pid\"",
        "\"unterminated",
    ] {
        assert!(parse_tasklist_output(123, true, output, "").is_err());
    }
}
