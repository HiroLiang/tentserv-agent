use super::*;

#[test]
fn toml_fixture_paths_round_trip_windows_prefixes_quotes_and_backslashes() {
    for path in [
        r"C:\Users\runner\model",
        r"\\?\C:\Users\runner\model",
        r"\\server\share\model",
        r#"/tmp/back\slash"quote'path"#,
    ] {
        let text = format!("source_path = {}\n", toml_path(path));
        let parsed: toml::Value = toml::from_str(&text).expect("valid fixture TOML");
        assert_eq!(parsed["source_path"].as_str(), Some(path));
    }
}

#[test]
fn metadata_fixture_writers_preserve_source_paths() {
    let home = unique_home("metadata-path-escaping");
    #[cfg(unix)]
    let home = home.join(r#"windows\style"path"#);
    let model_ref = "a".repeat(64);
    let safetensors_ref = "b".repeat(64);
    let adapter_ref = "c".repeat(64);
    let dataset_ref = "d".repeat(64);
    write_model_fixture(&home, &model_ref);
    write_safetensors_model_fixture_with_capabilities(&home, &safetensors_ref, &["chat"]);
    write_adapter_fixture(&home, &adapter_ref, &model_ref);
    write_dataset_fixture(&home, &dataset_ref);

    for (kind, reference, source) in [
        ("model", model_ref, "model"),
        ("model", safetensors_ref, "model"),
        ("adapter", adapter_ref, "adapter"),
        ("dataset", dataset_ref, "dataset"),
    ] {
        let metadata = home
            .join(format!("{kind}s/store"))
            .join(reference)
            .join(format!("{kind}.toml"));
        let text = fs::read_to_string(metadata).expect("fixture metadata");
        let parsed: toml::Value = toml::from_str(&text).expect("valid fixture TOML");
        assert_eq!(
            parsed["source_path"].as_str(),
            Some(path_string(home.join(format!("fixtures/{source}"))).as_str())
        );
    }
    fs::remove_dir_all(home).expect("remove fixture");
}
