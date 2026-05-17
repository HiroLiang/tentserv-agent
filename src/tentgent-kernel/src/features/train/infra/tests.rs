use std::path::PathBuf;

use crate::features::train::domain::TrainStoreLayout;
use crate::features::train::infra::StdTrainStoreLayoutInitializer;
use crate::features::train::ports::TrainStoreLayoutInitializer;

#[test]
fn train_layout_initializer_creates_standard_dirs() {
    let root = std::env::temp_dir().join(format!(
        "tentgent-kernel-train-infra-{}",
        std::process::id()
    ));
    let layout = TrainStoreLayout::from_train_dir(root.join("train"));

    StdTrainStoreLayoutInitializer
        .ensure_train_store_layout(&layout)
        .expect("ensure train layout");

    assert!(PathBuf::from(&layout.plans_dir).is_dir());
    assert!(PathBuf::from(&layout.staging_dir).is_dir());
}
