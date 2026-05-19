mod adapter;
mod auth;
mod capability;
mod dataset;
mod doctor;
mod model;
mod runtime;
mod server;
mod session;
mod train;

pub use adapter::AdapterKernelComponent;
pub use auth::AuthKernelComponent;
pub use capability::CapabilityKernelComponent;
pub use dataset::DatasetKernelComponent;
pub use doctor::DoctorKernelComponent;
pub use model::ModelKernelComponent;
pub use runtime::RuntimeKernelComponent;
pub use server::ServerKernelComponent;
pub use session::SessionKernelComponent;
pub use train::TrainKernelComponent;

use tentgent_kernel::{
    features::{
        adapter::{
            domain::AdapterRefSelector,
            usecases::{
                AdapterCatalogReadUseCase, AdapterInspectRequest, StdAdapterHfPullUseCase,
                StdAdapterTrainRunImportUseCase,
            },
        },
        chat::usecases::StdChatUseCase,
        daemon::infra::{StdDaemonKernel, DEFAULT_DAEMON_PROBE_TIMEOUT},
        dataset::usecases::{StdDatasetEvaluationUseCase, StdDatasetSynthesisUseCase},
        doctor::usecases::{
            DoctorReportUseCase, DoctorReportUseCaseRequest, DoctorReportUseCaseResult,
            StdDoctorRepairUseCase, StdDoctorReportUseCase,
        },
        embedding::usecases::StdEmbeddingUseCase,
        model::usecases::StdModelHfPullUseCase,
        rerank::usecases::StdRerankUseCase,
        server::{
            domain::ServerRefSelector,
            usecases::{ServerInspectRequest, ServerSpecUseCase, StdServerUseCase},
        },
        session::{
            domain::{SessionCompactionSummary, SessionStoreConfig},
            ports::{
                SessionAdapterRefResolutionRequest, SessionAdapterRefResolver, SessionPortFuture,
                SessionServerRefResolutionRequest, SessionServerRefResolver,
                SessionSummaryGenerationRequest, SessionSummaryGenerator,
            },
            usecases::StdSessionUseCase,
        },
        train::usecases::{StdLoraTrainPlanUseCase, StdLoraTrainRunUseCase},
    },
    foundation::{
        error::{KernelError, KernelResult},
        layout::{
            LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
        },
    },
};

use crate::bootstrap::DaemonBootstrapConfig;

pub struct KernelComponents {
    auth: AuthKernelComponent,
    capabilities: CapabilityKernelComponent,
    runtime: RuntimeKernelComponent,
    models: ModelKernelComponent,
    adapters: AdapterKernelComponent,
    datasets: DatasetKernelComponent,
    doctor: DoctorKernelComponent,
    servers: ServerKernelComponent,
    sessions: SessionKernelComponent,
    training: TrainKernelComponent,
    daemon: StdDaemonKernel,
}

impl KernelComponents {
    pub fn bootstrap(config: &DaemonBootstrapConfig) -> KernelResult<Self> {
        let layout = StdRuntimeLayoutResolver.resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: config.home.clone(),
            data_root_dir: None,
        })?;

        Ok(Self {
            auth: AuthKernelComponent::bootstrap(&layout)?,
            capabilities: CapabilityKernelComponent::new(),
            runtime: RuntimeKernelComponent::new(),
            models: ModelKernelComponent::new(),
            adapters: AdapterKernelComponent::new(),
            datasets: DatasetKernelComponent::new(),
            doctor: DoctorKernelComponent::new(),
            servers: ServerKernelComponent::new(),
            sessions: SessionKernelComponent::new(),
            training: TrainKernelComponent::new(),
            daemon: StdDaemonKernel::new(DEFAULT_DAEMON_PROBE_TIMEOUT)?,
        })
    }

    pub fn auth(&self) -> &AuthKernelComponent {
        &self.auth
    }

    pub fn capabilities(&self) -> &CapabilityKernelComponent {
        &self.capabilities
    }

    pub fn runtime(&self) -> &RuntimeKernelComponent {
        &self.runtime
    }

    pub fn models(&self) -> &ModelKernelComponent {
        &self.models
    }

    pub fn adapters(&self) -> &AdapterKernelComponent {
        &self.adapters
    }

    pub fn datasets(&self) -> &DatasetKernelComponent {
        &self.datasets
    }

    pub fn doctor(&self) -> &DoctorKernelComponent {
        &self.doctor
    }

    pub fn servers(&self) -> &ServerKernelComponent {
        &self.servers
    }

    pub fn sessions(&self) -> &SessionKernelComponent {
        &self.sessions
    }

    pub fn training(&self) -> &TrainKernelComponent {
        &self.training
    }

    pub fn daemon(&self) -> &StdDaemonKernel {
        &self.daemon
    }

    pub fn model_hf_pull_usecase(&self) -> StdModelHfPullUseCase<'_> {
        self.models.hf_pull_usecase(&self.runtime, &self.auth)
    }

    pub fn adapter_hf_pull_usecase(&self) -> StdAdapterHfPullUseCase<'_> {
        self.adapters
            .hf_pull_usecase(&self.runtime, &self.auth, self.models.catalog_store())
    }

    pub fn adapter_train_run_import_usecase(&self) -> StdAdapterTrainRunImportUseCase<'_> {
        self.adapters
            .train_run_import_usecase(self.models.catalog_store())
    }

    pub fn dataset_synthesis_usecase(&self) -> StdDatasetSynthesisUseCase<'_> {
        StdDatasetSynthesisUseCase::new(&self.runtime, &self.auth, &self.runtime)
    }

    pub fn dataset_evaluation_usecase(&self) -> StdDatasetEvaluationUseCase<'_> {
        self.datasets
            .evaluation_usecase(&self.runtime, &self.auth, &self.runtime)
    }

    pub fn chat_usecase(&self) -> StdChatUseCase<'_> {
        StdChatUseCase::new(&self.runtime, &self.models, &self.adapters, &self.runtime)
    }

    pub fn embedding_usecase(&self) -> StdEmbeddingUseCase<'_> {
        StdEmbeddingUseCase::new(&self.runtime, &self.models, &self.runtime)
    }

    pub fn rerank_usecase(&self) -> StdRerankUseCase<'_> {
        StdRerankUseCase::new(&self.runtime, &self.models, &self.runtime)
    }

    pub fn server_usecase(&self) -> StdServerUseCase<'_> {
        self.servers.usecase(self.models.catalog_store())
    }

    pub fn session_usecase(&self) -> StdSessionUseCase<'_> {
        self.sessions.usecase(self, self, self)
    }

    pub fn train_plan_usecase(&self) -> StdLoraTrainPlanUseCase<'_> {
        self.training
            .plan_usecase(self.models.catalog_store(), self.datasets.catalog_store())
    }

    pub fn train_run_usecase(&self) -> StdLoraTrainRunUseCase<'_> {
        self.training.run_usecase()
    }

    pub fn doctor_report_usecase(&self) -> StdDoctorReportUseCase<'_> {
        self.doctor
            .report_usecase(&self.runtime, &self.capabilities)
    }

    pub fn doctor_repair_usecase(&self) -> StdDoctorRepairUseCase<'_> {
        self.doctor.repair_usecase(&self.runtime, self)
    }
}

impl DoctorReportUseCase for KernelComponents {
    fn doctor_report(
        &self,
        request: DoctorReportUseCaseRequest,
    ) -> KernelResult<DoctorReportUseCaseResult> {
        self.doctor_report_usecase().doctor_report(request)
    }
}

impl SessionServerRefResolver for KernelComponents {
    fn resolve_session_server_ref(
        &self,
        request: SessionServerRefResolutionRequest,
    ) -> KernelResult<String> {
        let selector = ServerRefSelector::parse(&request.selector)
            .map_err(|err| session_kernel_error(format!("invalid server reference: {err}")))?;
        let inspected = self.server_usecase().inspect_server(ServerInspectRequest {
            layout: layout_input_from_session_store(&request.store)?,
            selector,
        })?;
        Ok(inspected.inspection.spec.server_ref.into_string())
    }
}

impl SessionAdapterRefResolver for KernelComponents {
    fn resolve_session_adapter_ref(
        &self,
        request: SessionAdapterRefResolutionRequest,
    ) -> KernelResult<String> {
        let selector = AdapterRefSelector::parse(&request.selector)
            .map_err(|err| session_kernel_error(format!("invalid adapter reference: {err}")))?;
        let inspected = self
            .adapters
            .catalog_usecase()
            .inspect_adapter(AdapterInspectRequest {
                layout: layout_input_from_session_store(&request.store)?,
                selector,
            })?;
        Ok(inspected.adapter.metadata.adapter_ref.into_string())
    }
}

impl SessionSummaryGenerator for KernelComponents {
    fn summarize_session(
        &self,
        _request: SessionSummaryGenerationRequest,
    ) -> SessionPortFuture<'_, SessionCompactionSummary> {
        Box::pin(async {
            Err(session_kernel_error(
                "daemon session summary generation must be handled by a chat handler",
            ))
        })
    }
}

fn layout_input_from_session_store(store: &SessionStoreConfig) -> KernelResult<RuntimeLayoutInput> {
    let home_dir = store
        .runtime_home_dir()
        .ok_or_else(|| {
            session_kernel_error("session store does not expose a runtime home for ref resolution")
        })?
        .to_path_buf();
    Ok(RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: Some(home_dir),
        data_root_dir: None,
    })
}

fn session_kernel_error(message: impl Into<String>) -> KernelError {
    KernelError::SessionStoreUnavailable(message.into())
}

#[cfg(test)]
mod tests;
