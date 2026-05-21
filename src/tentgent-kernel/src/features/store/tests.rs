use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::features::store::infra::FileStoreGarbageCollector;
use crate::features::store::usecases::{StdStoreGcUseCase, StoreGcRequest, StoreGcUseCase};
use crate::foundation::error::KernelResult;
use crate::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
};

#[test]
fn store_gc_dry_run_lists_staging_without_removing() {
    let home = unique_path("store-gc-dry-run");
    seed_staging(&home);
    let usecase = StdStoreGcUseCase::new(&FakeLayoutResolver, &FileStoreGarbageCollector);

    let result = usecase
        .gc_stores(StoreGcRequest {
            layout: layout_input(&home),
            apply: false,
        })
        .expect("gc stores");

    assert_eq!(result.outcome.items.len(), 3);
    assert_eq!(result.outcome.removed_count, 0);
    assert!(result.outcome.total_bytes >= 6);
    assert!(home.join("models/staging/pull-a").exists());

    let _ = fs::remove_dir_all(home);
}

#[test]
fn store_gc_apply_removes_only_staging_children() {
    let home = unique_path("store-gc-apply");
    seed_staging(&home);
    fs::create_dir_all(home.join("models/store/abcd")).expect("store dir");
    fs::write(home.join("models/store/abcd/model.toml"), b"keep").expect("store file");
    let usecase = StdStoreGcUseCase::new(&FakeLayoutResolver, &FileStoreGarbageCollector);

    let result = usecase
        .gc_stores(StoreGcRequest {
            layout: layout_input(&home),
            apply: true,
        })
        .expect("gc stores");

    assert_eq!(result.outcome.items.len(), 3);
    assert_eq!(result.outcome.removed_count, 3);
    assert!(!home.join("models/staging/pull-a").exists());
    assert!(!home.join("adapters/staging/pull-b").exists());
    assert!(!home.join("datasets/staging/add-c").exists());
    assert!(home.join("models/store/abcd/model.toml").exists());

    let _ = fs::remove_dir_all(home);
}

fn seed_staging(home: &std::path::Path) {
    fs::create_dir_all(home.join("models/staging/pull-a/source")).expect("model staging");
    fs::write(home.join("models/staging/pull-a/source/file.bin"), b"model").expect("model file");
    fs::create_dir_all(home.join("adapters/staging/pull-b/source")).expect("adapter staging");
    fs::write(
        home.join("adapters/staging/pull-b/source/adapter.bin"),
        b"adapter",
    )
    .expect("adapter file");
    fs::create_dir_all(home.join("datasets/staging/add-c/source")).expect("dataset staging");
    fs::write(home.join("datasets/staging/add-c/source/data.jsonl"), b"{}").expect("dataset file");
}

fn layout_input(home: &std::path::Path) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(home.to_path_buf()),
        data_root_dir: None,
    }
}

struct FakeLayoutResolver;

impl RuntimeLayoutResolver for FakeLayoutResolver {
    fn resolve(&self, input: RuntimeLayoutInput) -> KernelResult<RuntimeLayout> {
        let home = input.home_dir.expect("test home");
        Ok(RuntimeLayout {
            config_path: home.join("config.toml"),
            models_dir: home.join("models"),
            adapters_dir: home.join("adapters"),
            datasets_dir: home.join("datasets"),
            sessions_dir: home.join("sessions"),
            servers_dir: home.join("servers"),
            train_dir: home.join("train"),
            cache_dir: home.join("cache"),
            runtime_dir: home.join("runtime"),
            logs_dir: home.join("logs"),
            locks_dir: home.join("locks"),
            python_env_dir: home.join("runtime/python-env"),
            bootstrap_dir: home.join("runtime/bootstrap"),
            bootstrap_uv_dir: home.join("runtime/bootstrap/uv"),
            bootstrap_uv_cache_dir: home.join("runtime/bootstrap/uv-cache"),
            capabilities_path: home.join("runtime/capabilities.toml"),
            auth_metadata_path: home.join("runtime/auth.toml"),
            data_root_dir: home.clone(),
            home_dir: home,
        })
    }
}

fn unique_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "tentgent-kernel-{label}-{}-{nanos}",
        std::process::id()
    ))
}
