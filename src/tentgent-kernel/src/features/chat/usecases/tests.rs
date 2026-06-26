use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::features::adapter::domain::{
    AdapterBackendSupport, AdapterFormat, AdapterInspection, AdapterMetadata, AdapterRef,
    AdapterRefSelector, AdapterSourceKind, AdapterType,
};
use crate::features::chat::domain::{
    ChatBackend, ChatFinishReason, ChatGenerationOptions, ChatMessage, ChatPrompt, ChatRequest,
    ChatResponse, ChatRuntimeTarget, ChatStreamEvent, ResolvedChatAdapter, ResolvedChatTarget,
};
use crate::features::chat::ports::{
    ChatAdapterResolveRequest, ChatAdapterResolveResult, ChatAdapterResolver,
    ChatModelResolveRequest, ChatModelResolveResult, ChatModelResolver, ChatPortFuture,
    ChatRuntimeClient, ChatRuntimeRequest,
};
use crate::features::model::domain::{
    default_model_capability_source, ModelCapability, ModelCapabilityProofSource,
    ModelCapabilityProofStatus, ModelFormat, ModelInspection, ModelMetadata, ModelRef,
    ModelRefSelector, ModelSourceKind,
};
use crate::features::model::usecases::{
    ModelRuntimeExecutionEvidenceRecordRequest, ModelRuntimeExecutionEvidenceRecordResult,
    ModelRuntimeExecutionEvidenceRecorder,
};
use crate::features::runtime::domain::{
    PythonRuntimeLayout, PythonRuntimeResolutionInput, PythonRuntimeSource,
};
use crate::features::runtime::usecases::{
    RuntimeResolutionRequest, RuntimeResolutionResult, RuntimeResolutionUseCase,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::{LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput};

use super::port::{
    ChatCompletionResult, ChatCompletionUseCase, ChatPreparationRequest, ChatPreparationResult,
    ChatPreparationUseCase, ChatStreamingUseCase, ChatTargetSelection, ChatUseCaseFuture,
};
use super::StdChatUseCase;

#[tokio::test]
async fn chat_usecase_ports_cover_prepare_complete_and_stream_workflows() {
    let usecase = FakeChatUseCase;
    let request = chat_preparation_request();

    let prepared = usecase.prepare_chat(request.clone()).expect("prepare chat");
    assert_eq!(prepared.request.prompt.len(), 1);
    assert_eq!(
        prepared
            .model
            .as_ref()
            .map(|model| &model.metadata.model_ref),
        Some(&model_ref())
    );
    assert_eq!(
        prepared
            .adapter
            .as_ref()
            .map(|adapter| &adapter.metadata.adapter_ref),
        Some(&adapter_ref())
    );

    let completed = usecase
        .complete_chat(request.clone())
        .await
        .expect("complete chat");
    assert_eq!(completed.response.text, "done");
    assert_eq!(completed.response.finish_reason, ChatFinishReason::Stop);

    let mut events = Vec::new();
    let streamed = usecase
        .stream_chat(request, &mut |event| events.push(event))
        .await
        .expect("stream chat");
    assert_eq!(streamed.response.text, "streamed");
    assert_eq!(
        events,
        vec![
            ChatStreamEvent::Delta {
                text: "streamed".to_string()
            },
            ChatStreamEvent::Done {
                finish_reason: ChatFinishReason::Stop
            }
        ]
    );
}

#[tokio::test]
async fn standard_chat_usecase_prepares_completes_and_streams_local_adapter_chat() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let model_resolver = FakeChatModelResolver;
    let adapter_resolver = FakeChatAdapterResolver;
    let runtime_client = FakeChatRuntimeClient;
    let usecase = StdChatUseCase::new(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
    );

    let prepared = usecase
        .prepare_chat(chat_preparation_request())
        .expect("prepare chat");
    assert_eq!(
        prepared
            .model
            .as_ref()
            .map(|model| &model.metadata.model_ref),
        Some(&model_ref())
    );
    assert_eq!(
        prepared
            .adapter
            .as_ref()
            .map(|adapter| &adapter.metadata.adapter_ref),
        Some(&adapter_ref())
    );
    assert_eq!(
        prepared
            .request
            .target
            .adapter
            .as_ref()
            .map(|adapter| &adapter.adapter_ref),
        Some(&adapter_ref())
    );

    let completed = usecase
        .complete_chat(chat_preparation_request())
        .await
        .expect("complete chat");
    assert_eq!(completed.response.text, "runtime response");

    let mut events = Vec::new();
    let streamed = usecase
        .stream_chat(chat_preparation_request(), &mut |event| events.push(event))
        .await
        .expect("stream chat");
    assert_eq!(streamed.response.text, "runtime stream");
    assert_eq!(
        events,
        vec![
            ChatStreamEvent::Delta {
                text: "runtime stream".to_string()
            },
            ChatStreamEvent::Done {
                finish_reason: ChatFinishReason::Stop
            }
        ]
    );
}

#[test]
fn standard_chat_usecase_prepares_cloud_target_without_model_or_adapter() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let model_resolver = FakeChatModelResolver;
    let adapter_resolver = FakeChatAdapterResolver;
    let runtime_client = FakeChatRuntimeClient;
    let usecase = StdChatUseCase::new(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
    );
    let mut request = chat_preparation_request();
    request.target = ChatTargetSelection::CloudProvider {
        provider: crate::features::auth::domain::Provider::OpenAI,
        provider_model: "gpt-test".to_string(),
    };

    let prepared = usecase.prepare_chat(request).expect("prepare cloud chat");
    assert!(prepared.model.is_none());
    assert!(prepared.adapter.is_none());
    assert!(matches!(
        prepared.request.target.runtime,
        ChatRuntimeTarget::CloudProvider { .. }
    ));
    assert!(prepared.request.target.adapter.is_none());
}

#[tokio::test]
async fn standard_chat_usecase_records_verified_runtime_execution_proof() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let model_resolver = FakeChatModelResolver;
    let adapter_resolver = FakeChatAdapterResolver;
    let runtime_client = FakeChatRuntimeClient;
    let evidence = RecordingRuntimeEvidenceRecorder::default();
    let usecase = StdChatUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
        &evidence,
    );

    usecase
        .complete_chat(chat_preparation_request())
        .await
        .expect("complete chat");

    let records = evidence.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].capability, ModelCapability::Chat);
    assert_eq!(records[0].status, ModelCapabilityProofStatus::Verified);
    assert_eq!(records[0].error, None);
}

#[tokio::test]
async fn standard_chat_usecase_records_failed_runtime_execution_proof() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let model_resolver = FakeChatModelResolver;
    let adapter_resolver = FakeChatAdapterResolver;
    let runtime_client = FailingChatRuntimeClient;
    let evidence = RecordingRuntimeEvidenceRecorder::default();
    let usecase = StdChatUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
        &evidence,
    );

    let error = usecase
        .complete_chat(chat_preparation_request())
        .await
        .expect_err("runtime failure");

    assert!(error.to_string().contains("runtime failed"));
    let records = evidence.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].capability, ModelCapability::Chat);
    assert_eq!(records[0].status, ModelCapabilityProofStatus::Failed);
    assert!(records[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("runtime failed")));
}

#[tokio::test]
async fn standard_chat_usecase_does_not_record_cloud_runtime_failure_as_local_proof() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let model_resolver = FakeChatModelResolver;
    let adapter_resolver = FakeChatAdapterResolver;
    let runtime_client = FailingChatRuntimeClient;
    let evidence = RecordingRuntimeEvidenceRecorder::default();
    let usecase = StdChatUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
        &evidence,
    );
    let mut request = chat_preparation_request();
    request.target = ChatTargetSelection::CloudProvider {
        provider: crate::features::auth::domain::Provider::OpenAI,
        provider_model: "gpt-test".to_string(),
    };

    usecase
        .complete_chat(request)
        .await
        .expect_err("cloud runtime failure");

    assert!(evidence.records().is_empty());
}

#[tokio::test]
async fn standard_chat_usecase_does_not_record_prepare_failure_as_runtime_proof() {
    let runtime_resolution = FakeRuntimeResolutionUseCase;
    let model_resolver = FailingChatModelResolver;
    let adapter_resolver = FakeChatAdapterResolver;
    let runtime_client = FakeChatRuntimeClient;
    let evidence = RecordingRuntimeEvidenceRecorder::default();
    let usecase = StdChatUseCase::new_with_runtime_evidence(
        &runtime_resolution,
        &model_resolver,
        &adapter_resolver,
        &runtime_client,
        &evidence,
    );

    usecase
        .complete_chat(chat_preparation_request())
        .await
        .expect_err("prepare failure");

    assert!(evidence.records().is_empty());
}

struct FakeChatUseCase;

impl ChatPreparationUseCase for FakeChatUseCase {
    fn prepare_chat(&self, request: ChatPreparationRequest) -> KernelResult<ChatPreparationResult> {
        Ok(preparation_result(request))
    }
}

impl ChatCompletionUseCase for FakeChatUseCase {
    fn complete_chat(
        &'_ self,
        request: ChatPreparationRequest,
    ) -> ChatUseCaseFuture<'_, ChatCompletionResult> {
        Box::pin(async move {
            Ok(ChatCompletionResult {
                prepared: preparation_result(request),
                response: ChatResponse {
                    text: "done".to_string(),
                    finish_reason: ChatFinishReason::Stop,
                },
            })
        })
    }
}

impl ChatStreamingUseCase for FakeChatUseCase {
    fn stream_chat<'a>(
        &'a self,
        request: ChatPreparationRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatUseCaseFuture<'a, ChatCompletionResult> {
        Box::pin(async move {
            sink(ChatStreamEvent::Delta {
                text: "streamed".to_string(),
            });
            sink(ChatStreamEvent::Done {
                finish_reason: ChatFinishReason::Stop,
            });
            Ok(ChatCompletionResult {
                prepared: preparation_result(request),
                response: ChatResponse {
                    text: "streamed".to_string(),
                    finish_reason: ChatFinishReason::Stop,
                },
            })
        })
    }
}

struct FakeRuntimeResolutionUseCase;

impl RuntimeResolutionUseCase for FakeRuntimeResolutionUseCase {
    fn resolve_runtime(
        &self,
        request: RuntimeResolutionRequest,
    ) -> KernelResult<RuntimeResolutionResult> {
        let home = request
            .layout
            .home_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("/tmp/tentgent-chat-usecase"));
        let runtime_project = request
            .runtime
            .project_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("/tmp/tentgent-python-project"));
        let runtime_env = request
            .runtime
            .python_env_dir
            .as_deref()
            .unwrap_or_else(|| Path::new("/tmp/tentgent-python-env"));

        Ok(RuntimeResolutionResult {
            layout: runtime_layout(home),
            runtime: PythonRuntimeLayout {
                project_dir: runtime_project.to_path_buf(),
                env_dir: runtime_env.to_path_buf(),
                source: PythonRuntimeSource::DevelopmentSource,
            },
        })
    }
}

struct FakeChatModelResolver;

impl ChatModelResolver for FakeChatModelResolver {
    fn resolve_chat_model(
        &self,
        request: ChatModelResolveRequest,
    ) -> KernelResult<ChatModelResolveResult> {
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or_else(|| Path::new("/tmp/tentgent-chat-usecase")),
        );
        Ok(ChatModelResolveResult {
            model: model_inspection(&layout),
            target: ChatRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: ChatBackend::TransformersPeft,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::Chat],
            },
            layout,
        })
    }
}

struct FailingChatModelResolver;

impl ChatModelResolver for FailingChatModelResolver {
    fn resolve_chat_model(
        &self,
        _request: ChatModelResolveRequest,
    ) -> KernelResult<ChatModelResolveResult> {
        Err(KernelError::ModelStoreUnavailable(
            "model lookup failed".to_string(),
        ))
    }
}

struct FakeChatAdapterResolver;

impl ChatAdapterResolver for FakeChatAdapterResolver {
    fn resolve_chat_adapter(
        &self,
        request: ChatAdapterResolveRequest,
    ) -> KernelResult<ChatAdapterResolveResult> {
        assert_eq!(
            request.target.backend,
            AdapterBackendSupport::TransformersPeft
        );
        let layout = runtime_layout(
            request
                .layout
                .home_dir
                .as_deref()
                .unwrap_or_else(|| Path::new("/tmp/tentgent-chat-usecase")),
        );
        let adapter = adapter_inspection(&layout);
        Ok(ChatAdapterResolveResult {
            target: ResolvedChatAdapter {
                adapter_ref: adapter.metadata.adapter_ref.clone(),
                backend: AdapterBackendSupport::TransformersPeft,
                source_path: adapter.source_path.clone(),
            },
            adapter,
            layout,
        })
    }
}

struct FakeChatRuntimeClient;

impl ChatRuntimeClient for FakeChatRuntimeClient {
    fn generate_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move {
            assert_eq!(request.request.prompt.len(), 1);
            Ok(ChatResponse {
                text: "runtime response".to_string(),
                finish_reason: ChatFinishReason::Stop,
            })
        })
    }

    fn stream_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move {
            assert_eq!(request.request.prompt.len(), 1);
            sink(ChatStreamEvent::Delta {
                text: "runtime stream".to_string(),
            });
            sink(ChatStreamEvent::Done {
                finish_reason: ChatFinishReason::Stop,
            });
            Ok(ChatResponse {
                text: "runtime stream".to_string(),
                finish_reason: ChatFinishReason::Stop,
            })
        })
    }
}

struct FailingChatRuntimeClient;

impl ChatRuntimeClient for FailingChatRuntimeClient {
    fn generate_chat<'a>(
        &'a self,
        _request: ChatRuntimeRequest,
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move { Err(KernelError::ChatRuntimeUnavailable("runtime failed".into())) })
    }

    fn stream_chat<'a>(
        &'a self,
        _request: ChatRuntimeRequest,
        _sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move { Err(KernelError::ChatRuntimeUnavailable("runtime failed".into())) })
    }
}

#[derive(Default)]
struct RecordingRuntimeEvidenceRecorder {
    records: Mutex<Vec<ModelRuntimeExecutionEvidenceRecordRequest>>,
}

impl RecordingRuntimeEvidenceRecorder {
    fn records(&self) -> Vec<ModelRuntimeExecutionEvidenceRecordRequest> {
        self.records.lock().expect("records lock").clone()
    }
}

impl ModelRuntimeExecutionEvidenceRecorder for RecordingRuntimeEvidenceRecorder {
    fn record_runtime_execution_evidence(
        &self,
        request: ModelRuntimeExecutionEvidenceRecordRequest,
    ) -> KernelResult<ModelRuntimeExecutionEvidenceRecordResult> {
        let metadata = request.metadata.clone();
        self.records
            .lock()
            .expect("records lock")
            .push(request.clone());
        Ok(ModelRuntimeExecutionEvidenceRecordResult {
            proof: crate::features::model::domain::ModelCapabilityProof {
                model_ref: metadata.model_ref,
                capability: request.capability,
                status: request.status,
                source: ModelCapabilityProofSource::RuntimeExecution,
                primary_format: metadata.primary_format,
                mlx_runtime_family: metadata.mlx_runtime_family,
                backend: "safetensors".to_string(),
                runtime_version: None,
                runtime_profile: request.runtime_profile,
                runtime_profile_version: request.runtime_profile_version,
                server_ref: request.server_ref,
                checked_at: "2026-06-12T00:00:00Z".to_string(),
                error: request.error,
            },
        })
    }
}

fn chat_preparation_request() -> ChatPreparationRequest {
    ChatPreparationRequest {
        layout: RuntimeLayoutInput {
            mode: LayoutResolveMode::ReadOnly,
            home_dir: Some(PathBuf::from("/tmp/tentgent-chat-usecase")),
            data_root_dir: None,
        },
        runtime: PythonRuntimeResolutionInput {
            project_dir: Some(PathBuf::from("/tmp/tentgent-python-project")),
            python_env_dir: Some(PathBuf::from("/tmp/tentgent-python-env")),
        },
        target: ChatTargetSelection::LocalModel {
            model_selector: ModelRefSelector::parse(model_ref().short_ref())
                .expect("model selector"),
            adapter_selector: Some(
                AdapterRefSelector::parse(adapter_ref().short_ref()).expect("adapter selector"),
            ),
        },
        prompt: ChatPrompt::new(vec![ChatMessage::user("hello").expect("message")])
            .expect("prompt"),
        options: ChatGenerationOptions {
            max_tokens: Some(16),
            temperature: Some(0.1),
            stream: false,
        },
    }
}

fn preparation_result(request: ChatPreparationRequest) -> ChatPreparationResult {
    let home = request
        .layout
        .home_dir
        .as_deref()
        .unwrap_or_else(|| Path::new("/tmp/tentgent-chat-usecase"));
    let runtime_project = request
        .runtime
        .project_dir
        .as_deref()
        .unwrap_or_else(|| Path::new("/tmp/tentgent-python-project"));
    let runtime_env = request
        .runtime
        .python_env_dir
        .as_deref()
        .unwrap_or_else(|| Path::new("/tmp/tentgent-python-env"));

    let layout = runtime_layout(home);
    let runtime = PythonRuntimeLayout {
        project_dir: runtime_project.to_path_buf(),
        env_dir: runtime_env.to_path_buf(),
        source: PythonRuntimeSource::DevelopmentSource,
    };
    let model = model_inspection(&layout);
    let adapter = adapter_inspection(&layout);
    let request = ChatRequest {
        target: ResolvedChatTarget {
            runtime: ChatRuntimeTarget::LocalModel {
                model_ref: model_ref(),
                backend: ChatBackend::TransformersPeft,
                source_repo: Some("org/model".to_string()),
                source_revision: Some("main".to_string()),
                model_capabilities: vec![ModelCapability::Chat],
            },
            adapter: Some(ResolvedChatAdapter {
                adapter_ref: adapter_ref(),
                backend: AdapterBackendSupport::TransformersPeft,
                source_path: adapter.source_path.clone(),
            }),
        },
        prompt: request.prompt,
        options: request.options,
    };

    ChatPreparationResult {
        layout,
        runtime,
        model: Some(model),
        adapter: Some(adapter),
        request: ChatRequest {
            prompt: request.prompt,
            options: request.options,
            target: request.target,
        },
    }
}

fn runtime_layout(home: &Path) -> RuntimeLayout {
    RuntimeLayout {
        home_dir: home.to_path_buf(),
        data_root_dir: home.to_path_buf(),
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
        python_env_dir: home.join("python"),
        bootstrap_dir: home.join("bootstrap"),
        bootstrap_uv_dir: home.join("bootstrap").join("uv"),
        bootstrap_uv_cache_dir: home.join("bootstrap").join("uv-cache"),
        capabilities_path: home.join("runtime").join("capabilities.toml"),
        auth_metadata_path: home.join("runtime").join("auth.toml"),
    }
}

fn model_inspection(layout: &RuntimeLayout) -> ModelInspection {
    ModelInspection {
        metadata: ModelMetadata {
            model_ref: model_ref(),
            short_ref: model_ref().short_ref().to_string(),
            source_kind: ModelSourceKind::HuggingFace,
            source_repo: Some("org/model".to_string()),
            source_revision: Some("main".to_string()),
            source_path: None,
            primary_format: ModelFormat::Safetensors,
            detected_formats: vec![ModelFormat::Safetensors],
            mlx_runtime_family: None,
            model_capabilities: vec![ModelCapability::Chat],
            model_capability_source: default_model_capability_source(),
            file_count: 1,
            total_bytes: 10,
            imported_at: "2026-01-01T00:00:00Z".to_string(),
        },
        store_path: layout.models_dir.join("store").join(model_ref().as_str()),
        manifest_path: layout
            .models_dir
            .join("store")
            .join(model_ref().as_str())
            .join("manifest.json"),
        variant_source_path: layout
            .models_dir
            .join("store")
            .join(model_ref().as_str())
            .join("source"),
    }
}

fn adapter_inspection(layout: &RuntimeLayout) -> AdapterInspection {
    AdapterInspection {
        metadata: AdapterMetadata {
            adapter_ref: adapter_ref(),
            short_ref: adapter_ref().short_ref().to_string(),
            adapter_format: AdapterFormat::Peft,
            adapter_type: AdapterType::Lora,
            target_capability: Some(ModelCapability::Chat),
            base_model_ref: Some(model_ref()),
            base_model_source_repo: Some("org/model".to_string()),
            base_model_source_revision: Some("main".to_string()),
            model_family: None,
            backend_support: vec![AdapterBackendSupport::TransformersPeft],
            control_kind: None,
            weight_file: None,
            trigger_words: Vec::new(),
            recommended_scale: None,
            source_kind: AdapterSourceKind::Local,
            source_repo: None,
            source_revision: None,
            source_path: Some("/tmp/adapter/source".to_string()),
            training_dataset_ref: None,
            training_run_ref: None,
            training_config_ref: None,
            file_count: 1,
            total_bytes: 10,
            imported_at: "2026-01-01T00:00:00Z".to_string(),
        },
        store_path: layout
            .adapters_dir
            .join("store")
            .join(adapter_ref().as_str()),
        manifest_path: layout
            .adapters_dir
            .join("store")
            .join(adapter_ref().as_str())
            .join("manifest.json"),
        source_path: layout
            .adapters_dir
            .join("store")
            .join(adapter_ref().as_str())
            .join("source"),
    }
}

fn model_ref() -> ModelRef {
    ModelRef::parse("a".repeat(64)).expect("model ref")
}

fn adapter_ref() -> AdapterRef {
    AdapterRef::parse("b".repeat(64)).expect("adapter ref")
}
