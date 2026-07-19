use std::{
    fs,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use crate::{
    features::resource_coordination::{
        ResourceCoordinationCode, ResourceCoordinator, ResourceKey, ResourceKind, ResourceLockMode,
        ResourceLockRequest,
    },
    foundation::layout::RuntimeLayout,
};

use super::FileResourceCoordinator;

#[test]
fn fs2_lock_contention_is_classified_as_busy() {
    assert!(super::file_coordinator::is_lock_contended(
        &fs2::lock_contended_error()
    ));
    assert!(!super::file_coordinator::is_lock_contended(
        &std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied")
    ));
}

#[test]
fn coordination_codes_keep_compatible_serialized_values() {
    assert_eq!(
        serde_json::to_string(&ResourceCoordinationCode::ResourceBusy).unwrap(),
        "\"resource-busy\""
    );
    assert_eq!(
        serde_json::to_string(&ResourceCoordinationCode::ResourceStateUnstable).unwrap(),
        "\"resource-state-unstable\""
    );
}

#[test]
fn shared_holders_coexist_and_exclusive_times_out() {
    let root = temp_root("shared-exclusive");
    let layout = runtime_layout(&root);
    let coordinator = FileResourceCoordinator;
    let key = ResourceKey::new(ResourceKind::Model, "model-a");
    let first = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new("read-a", vec![(key.clone(), ResourceLockMode::Shared)]),
        )
        .unwrap()
        .unwrap();
    let second = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new("read-b", vec![(key.clone(), ResourceLockMode::Shared)]),
        )
        .unwrap()
        .unwrap();
    let busy = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new("delete", vec![(key, ResourceLockMode::Exclusive)])
                .with_limits(Duration::from_millis(20), 2),
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(busy.code, ResourceCoordinationCode::ResourceBusy);
    assert_eq!(busy.holders.len(), 2);
    drop(second);
    drop(first);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn partial_multi_resource_acquisition_is_released_before_retry() {
    let root = temp_root("partial-release");
    let layout = runtime_layout(&root);
    let coordinator = FileResourceCoordinator;
    let first_key = ResourceKey::new(ResourceKind::Model, "a");
    let blocked_key = ResourceKey::new(ResourceKind::Model, "b");
    let _blocker = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "hold-b",
                vec![(blocked_key.clone(), ResourceLockMode::Exclusive)],
            ),
        )
        .unwrap()
        .unwrap();
    let busy = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "hold-a-b",
                vec![
                    (blocked_key, ResourceLockMode::Exclusive),
                    (first_key.clone(), ResourceLockMode::Exclusive),
                ],
            )
            .with_limits(Duration::from_millis(20), 1),
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(busy.code, ResourceCoordinationCode::ResourceBusy);
    assert!(super::holder_metadata::read_holders(&layout, &first_key)
        .unwrap()
        .is_empty());
    let _first = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new("hold-a", vec![(first_key, ResourceLockMode::Exclusive)])
                .with_limits(Duration::from_millis(20), 1),
        )
        .unwrap()
        .expect("partially acquired lock must have been released");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn process_exit_releases_advisory_lock() {
    let root = temp_root("process-exit");
    let marker = root.join("ready");
    fs::create_dir_all(&root).expect("test root");
    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--ignored",
            "--exact",
            "features::resource_coordination::infra::tests::subprocess_lock_holder",
            "--nocapture",
        ])
        .env("TENTGENT_COORDINATION_TEST_ROOT", &root)
        .spawn()
        .expect("spawn lock holder");
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "subprocess did not acquire lock");

    let layout = runtime_layout(&root);
    let key = ResourceKey::new(ResourceKind::Model, "subprocess");
    let busy = FileResourceCoordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "parent-contention",
                vec![(key.clone(), ResourceLockMode::Exclusive)],
            )
            .with_limits(Duration::from_millis(30), 2),
        )
        .unwrap()
        .unwrap_err();
    assert_eq!(busy.code, ResourceCoordinationCode::ResourceBusy);
    assert!(child.wait().expect("wait lock holder").success());
    let _permit = FileResourceCoordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "parent-after-exit",
                vec![(key, ResourceLockMode::Exclusive)],
            )
            .with_limits(Duration::from_millis(100), 4),
        )
        .unwrap()
        .expect("process exit must release advisory lock");
    let _ = fs::remove_dir_all(root);
}

#[test]
#[ignore = "subprocess helper"]
fn subprocess_lock_holder() {
    let Some(root) = std::env::var_os("TENTGENT_COORDINATION_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let layout = runtime_layout(&root);
    let _permit = FileResourceCoordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "subprocess-holder",
                vec![(
                    ResourceKey::new(ResourceKind::Model, "subprocess"),
                    ResourceLockMode::Exclusive,
                )],
            ),
        )
        .unwrap()
        .unwrap();
    fs::write(root.join("ready"), b"ready").expect("write ready marker");
    thread::sleep(Duration::from_millis(400));
}

#[test]
fn disjoint_keys_do_not_block() {
    let root = temp_root("disjoint");
    let layout = runtime_layout(&root);
    let coordinator = FileResourceCoordinator;
    let _first = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "delete-a",
                vec![(
                    ResourceKey::new(ResourceKind::Model, "a"),
                    ResourceLockMode::Exclusive,
                )],
            ),
        )
        .unwrap()
        .unwrap();
    let _second = coordinator
        .acquire(
            &layout,
            ResourceLockRequest::new(
                "delete-b",
                vec![(
                    ResourceKey::new(ResourceKind::Model, "b"),
                    ResourceLockMode::Exclusive,
                )],
            ),
        )
        .unwrap()
        .unwrap();
    let _ = fs::remove_dir_all(root);
}

fn temp_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-resource-coordination-{label}-{}",
        crate::features::resource_coordination::new_operation_id()
    ))
}

fn runtime_layout(root: &std::path::Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: root.to_path_buf(),
        data_root_dir: root.join("data"),
        models_dir: root.join("models"),
        servers_dir: root.join("servers"),
        adapters_dir: root.join("adapters"),
        datasets_dir: root.join("datasets"),
        sessions_dir: root.join("sessions"),
        train_dir: root.join("train"),
        cache_dir: root.join("cache"),
        runtime_dir: root.join("runtime"),
        logs_dir: root.join("logs"),
        locks_dir: root.join("locks"),
        python_env_dir: root.join("runtime/python"),
        bootstrap_dir: root.join("runtime/bootstrap"),
        bootstrap_uv_dir: root.join("runtime/bootstrap/uv"),
        bootstrap_uv_cache_dir: root.join("runtime/bootstrap/uv-cache"),
        capabilities_path: root.join("runtime/capabilities.toml"),
        auth_metadata_path: root.join("runtime/auth.toml"),
        config_path: root.join("config.toml"),
    }
}
