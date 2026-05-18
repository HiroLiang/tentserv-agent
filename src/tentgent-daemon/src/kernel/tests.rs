use std::{fs, path::PathBuf};

use crate::bootstrap::{DaemonBootstrapConfig, LoggingConfig, RestConfig};

use super::KernelComponents;

#[test]
fn bootstrap_builds_kernel_component_registry() {
    let home = unique_home("kernel-components");
    let config = DaemonBootstrapConfig {
        home: Some(home.clone()),
        logging: LoggingConfig {
            enabled: false,
            env_filter: None,
        },
        rest: RestConfig::default(),
    };

    let components = KernelComponents::bootstrap(&config).expect("kernel components");
    let _ = components.auth().status_usecase();
    let _ = components.capabilities().resolver_usecase();
    let _ = components.runtime().resolution_usecase();
    let _ = components.models().catalog_usecase();
    let _ = components.adapters().catalog_usecase();
    let _ = components.datasets().catalog_usecase();
    let _ = components.server_usecase();
    let _ = components.session_usecase();
    let _ = components.train_plan_usecase();
    let _ = components.train_run_usecase();
    let _ = components.doctor_report_usecase();
    let _ = components.chat_usecase();
    let _ = components.daemon().usecase();

    let _ = fs::remove_dir_all(home);
}

fn unique_home(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-daemon-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}
