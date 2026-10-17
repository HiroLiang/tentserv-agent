use super::{
    model_file_diagnostics, model_metadata_for, temp_model_store, write_file, write_model_skeleton,
    ModelCapability, ModelFileDiagnosticCode, ModelFileDiagnosticSeverity, ModelFormat,
    ModelMetadata, ModelRef, ModelStoreLayout, ModelVariantMetadata,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn accepts_primary_gguf_with_projector_sidecars() {
    for projectors in [
        vec!["mmproj-model-f16-4B.gguf"],
        vec!["mmproj.gguf", "MMPROJ-F16.GGUF", "mmproj_q8.gguf"],
    ] {
        let (layout, metadata, source) = fixture("gguf-with-projectors");
        write_file(source.join("gemma-3-4b-it-q4_0.gguf"), "model");
        for projector in projectors {
            write_file(source.join(projector), "projector");
        }

        assert!(model_file_diagnostics(&layout, &metadata).is_empty());
    }
}

#[test]
fn rejects_projector_only_sources() {
    for name in ["mmproj.gguf", "mmproj-model-f16-4B.gguf", "MMPROJ_F16.GGUF"] {
        let (layout, metadata, source) = fixture("projector-only");
        write_file(source.join(name), "projector");

        assert_blocking(&layout, &metadata, ModelFileDiagnosticCode::MissingGgufFile);
    }
}

#[test]
fn rejects_explicit_projector_source() {
    let (layout, metadata, source) = fixture("explicit-projector");
    write_file(source.join("mmproj-model-f16-4B.gguf"), "projector");
    select_source_file(&layout, &metadata, "mmproj-model-f16-4B.gguf");

    assert_blocking(&layout, &metadata, ModelFileDiagnosticCode::MissingGgufFile);
}

#[test]
fn accepts_explicit_primary_gguf_source() {
    let (layout, metadata, source) = fixture("explicit-primary");
    write_file(source.join("MODEL.GGUF"), "model");
    select_source_file(&layout, &metadata, "MODEL.GGUF");

    assert!(model_file_diagnostics(&layout, &metadata).is_empty());
}

#[test]
fn rejects_multiple_primary_files_even_with_projectors() {
    let (layout, metadata, source) = fixture("multiple-primary");
    for name in ["model-q4.gguf", "model-q8.gguf", "mmproj-f16.gguf"] {
        write_file(source.join(name), "fixture");
    }

    assert_blocking(
        &layout,
        &metadata,
        ModelFileDiagnosticCode::MultipleGgufFiles,
    );
}

#[test]
fn does_not_treat_similar_names_as_projectors() {
    for name in ["mmproject.gguf", "model-mmproj.gguf"] {
        let (layout, metadata, source) = fixture("similar-projector-name");
        write_file(source.join("model.gguf"), "model");
        write_file(source.join(name), "another model");

        assert_blocking(
            &layout,
            &metadata,
            ModelFileDiagnosticCode::MultipleGgufFiles,
        );
    }
}

#[test]
fn accepts_uppercase_extension_and_ignores_gguf_directories() {
    let (layout, metadata, source) = fixture("uppercase-primary");
    write_file(source.join("MODEL.GGUF"), "model");
    fs::create_dir_all(source.join("nested.gguf")).expect("GGUF-named directory");
    write_file(source.join("nested.gguf/other.gguf"), "nested model");

    assert!(model_file_diagnostics(&layout, &metadata).is_empty());
}

#[test]
fn nested_gguf_is_not_a_primary_model() {
    let (layout, metadata, source) = fixture("nested-primary");
    write_file(source.join("nested.gguf/model.gguf"), "nested model");

    assert_blocking(&layout, &metadata, ModelFileDiagnosticCode::MissingGgufFile);
}

fn fixture(label: &str) -> (ModelStoreLayout, ModelMetadata, PathBuf) {
    let layout = temp_model_store(label);
    let metadata = model_metadata_for(
        ModelRef::parse("a".repeat(64)).expect("model ref"),
        ModelFormat::Gguf,
        vec![ModelCapability::Chat],
    );
    write_model_skeleton(&layout, &metadata);
    let source = layout.variant_source_dir(&metadata.model_ref, metadata.primary_format);
    (layout, metadata, source)
}

fn select_source_file(layout: &ModelStoreLayout, metadata: &ModelMetadata, filename: &str) {
    let path = layout.variant_metadata_path(&metadata.model_ref, metadata.primary_format);
    let mut variant: ModelVariantMetadata =
        toml::from_str(&fs::read_to_string(&path).expect("variant metadata"))
            .expect("parse variant");
    variant.relative_source_path = format!("{}/{filename}", variant.relative_source_path);
    fs::write(path, toml::to_string(&variant).expect("serialize variant")).expect("write variant");
}

fn assert_blocking(
    layout: &ModelStoreLayout,
    metadata: &ModelMetadata,
    code: ModelFileDiagnosticCode,
) {
    let diagnostics = model_file_diagnostics(layout, metadata);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, code);
    assert_eq!(
        diagnostics[0].severity,
        ModelFileDiagnosticSeverity::Blocking
    );
    assert!(diagnostics[0].message.contains("primary .gguf model file"));
    let next_action = &diagnostics[0].next_action;
    assert!(next_action.contains("exactly one primary GGUF model"));
    assert!(next_action.contains("tentgent model add <source-dir>"));
    assert!(!next_action.contains("corrupted"));
    assert!(!next_action.contains("pull"));
}
