use super::*;

pub(super) struct Fixture {
    home: PathBuf,
    data: PathBuf,
    pub(super) model_ref: ModelRef,
}

impl Fixture {
    pub(super) fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tentgent-kernel-server-usecase-{label}-{}-{nanos}",
            std::process::id()
        ));
        Self {
            home: root.join("home"),
            data: root.join("data"),
            model_ref: ModelRef::parse(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .expect("model ref"),
        }
    }

    pub(super) fn layout_input(&self, mode: LayoutResolveMode) -> RuntimeLayoutInput {
        RuntimeLayoutInput {
            mode,
            home_dir: Some(self.home.clone()),
            data_root_dir: Some(self.data.clone()),
        }
    }

    pub(super) fn write_chat_model(&self) {
        self.write_model_capabilities(default_model_capabilities());
    }

    pub(super) fn write_model_capabilities(&self, capabilities: Vec<ModelCapability>) {
        self.write_model_format_capabilities(ModelFormat::Safetensors, capabilities);
    }

    pub(super) fn write_model_format_capabilities(
        &self,
        format: ModelFormat,
        capabilities: Vec<ModelCapability>,
    ) {
        self.write_model_metadata(format, capabilities, ModelSourceKind::Local, None);
    }

    pub(super) fn write_hf_model_format_capabilities(
        &self,
        format: ModelFormat,
        capabilities: Vec<ModelCapability>,
        source_repo: &str,
    ) {
        self.write_model_metadata(
            format,
            capabilities,
            ModelSourceKind::HuggingFace,
            Some(source_repo.to_string()),
        );
    }

    pub(super) fn write_hf_mlx_model_capabilities(
        &self,
        capabilities: Vec<ModelCapability>,
        source_repo: &str,
        mlx_runtime_family: MlxRuntimeFamily,
    ) {
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        let stored_capabilities = capabilities.clone();
        FileModelCatalogStore
            .save_model_metadata(
                &model_store,
                &ModelMetadata {
                    model_ref: self.model_ref.clone(),
                    short_ref: self.model_ref.short_ref().to_string(),
                    source_kind: ModelSourceKind::HuggingFace,
                    source_repo: Some(source_repo.to_string()),
                    source_revision: None,
                    source_path: Some("/tmp/model".to_string()),
                    primary_format: ModelFormat::Mlx,
                    detected_formats: vec![ModelFormat::Mlx],
                    mlx_runtime_family: Some(mlx_runtime_family),
                    model_capabilities: capabilities,
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 1024,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                },
            )
            .expect("save model");
        self.write_model_files(&model_store, ModelFormat::Mlx, &stored_capabilities);
    }

    pub(super) fn write_model_metadata(
        &self,
        format: ModelFormat,
        capabilities: Vec<ModelCapability>,
        source_kind: ModelSourceKind,
        source_repo: Option<String>,
    ) {
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        let stored_capabilities = capabilities.clone();
        FileModelCatalogStore
            .save_model_metadata(
                &model_store,
                &ModelMetadata {
                    model_ref: self.model_ref.clone(),
                    short_ref: self.model_ref.short_ref().to_string(),
                    source_kind,
                    source_repo,
                    source_revision: None,
                    source_path: Some("/tmp/model".to_string()),
                    primary_format: format,
                    detected_formats: vec![format],
                    mlx_runtime_family: None,
                    model_capabilities: capabilities,
                    model_capability_source: default_model_capability_source(),
                    file_count: 1,
                    total_bytes: 1024,
                    imported_at: "2026-05-17T00:00:00Z".to_string(),
                },
            )
            .expect("save model");
        self.write_model_files(&model_store, format, &stored_capabilities);
    }

    pub(super) fn write_model_files(
        &self,
        model_store: &ModelStoreLayout,
        format: ModelFormat,
        capabilities: &[ModelCapability],
    ) {
        let catalog = FileModelCatalogStore;
        catalog
            .save_model_manifest(
                model_store,
                &self.model_ref,
                &ModelManifest {
                    files: vec![ModelManifestEntry {
                        relative_path: "source/config.json".to_string(),
                        size_bytes: 2,
                        sha256: "0".repeat(64),
                    }],
                },
            )
            .expect("save manifest");
        catalog
            .save_variant_metadata(
                model_store,
                &self.model_ref,
                &ModelVariantMetadata {
                    format,
                    status: ModelVariantStatus::Imported,
                    import_method: ModelImportMethod::Add,
                    relative_source_path: SOURCE_DIRNAME.to_string(),
                },
            )
            .expect("save variant");

        let source = model_store.variant_source_dir(&self.model_ref, format);
        std::fs::create_dir_all(&source).expect("create model source");
        match format {
            ModelFormat::Gguf => write_fixture_file(source.join("model.gguf"), "gguf"),
            ModelFormat::Diffusers => write_fixture_file(source.join("model_index.json"), "{}"),
            ModelFormat::Safetensors | ModelFormat::Mlx => {
                write_fixture_file(source.join("config.json"), "{}");
                if capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        ModelCapability::Chat
                            | ModelCapability::Embedding
                            | ModelCapability::Rerank
                    )
                }) {
                    write_fixture_file(source.join("tokenizer.json"), "{}");
                }
                if capabilities.iter().any(|capability| {
                    matches!(
                        capability,
                        ModelCapability::AudioTranscription
                            | ModelCapability::AudioSpeech
                            | ModelCapability::VisionChat
                            | ModelCapability::VideoUnderstanding
                    )
                }) {
                    write_fixture_file(source.join("processor_config.json"), "{}");
                }
            }
        }
    }

    pub(super) fn write_capability_proof(
        &self,
        capability: ModelCapability,
        status: ModelCapabilityProofStatus,
        backend: &str,
        error: Option<&str>,
    ) {
        let (runtime_profile, runtime_profile_version) =
            if capability == ModelCapability::Chat && backend == "safetensors" {
                (Some("local-chat-transformers-peft".to_string()), Some(1))
            } else {
                (None, None)
            };
        let layout = StdRuntimeLayoutResolver
            .resolve(self.layout_input(LayoutResolveMode::Create))
            .expect("layout");
        let model_store = ModelStoreLayout::from_models_dir(layout.models_dir);
        FileModelCapabilityProofStore
            .save_capability_proof(
                &model_store,
                &ModelCapabilityProof {
                    model_ref: self.model_ref.clone(),
                    capability,
                    status,
                    source: ModelCapabilityProofSource::ServerStart,
                    primary_format: ModelFormat::Safetensors,
                    mlx_runtime_family: None,
                    backend: backend.to_string(),
                    runtime_version: None,
                    runtime_profile,
                    runtime_profile_version,
                    server_ref: Some("server-ref".to_string()),
                    checked_at: "2026-05-17T00:00:00Z".to_string(),
                    error: error.map(str::to_string),
                },
            )
            .expect("save proof");
    }
}

pub(super) fn cluster_definition(
    cluster_ref: ClusterRef,
    model_ref: ModelRef,
    include_optional_route: bool,
) -> ClusterDefinition {
    let mut routes = [(
        ClusterRouteKey::Chat,
        ClusterRouteTarget::LocalModel {
            model_ref: model_ref.clone(),
            runtime_profile: None,
        },
    )]
    .into_iter()
    .collect::<std::collections::BTreeMap<_, _>>();
    if include_optional_route {
        routes.insert(
            ClusterRouteKey::Embedding,
            ClusterRouteTarget::LocalModel {
                model_ref,
                runtime_profile: None,
            },
        );
    }
    ClusterDefinition {
        schema_version: CLUSTER_SCHEMA_VERSION,
        cluster_ref,
        route_update_policy: Default::default(),
        routes,
    }
}

pub(super) fn write_fixture_file(path: PathBuf, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent");
    }
    std::fs::write(path, body).expect("write fixture file");
}

pub(super) struct StaticClock;

impl ServerClock for StaticClock {
    fn now_rfc3339(&self) -> KernelResult<String> {
        Ok("2026-05-17T00:00:00Z".to_string())
    }
}

pub(super) struct StaticProcessProbe {
    pub(super) running: bool,
}

impl ServerProcessProbe for StaticProcessProbe {
    fn is_process_running(&self, _pid: u32) -> KernelResult<bool> {
        Ok(self.running)
    }
}

pub(super) struct StaticProcessController;

impl ServerProcessController for StaticProcessController {
    fn terminate_process(&self, _pid: u32) -> KernelResult<()> {
        Ok(())
    }
}
