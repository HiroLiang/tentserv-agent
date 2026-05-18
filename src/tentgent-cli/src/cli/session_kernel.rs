use std::path::{Path, PathBuf};

use tentgent_kernel::features::adapter::{
    domain::AdapterRefSelector,
    infra::FileAdapterCatalogStore,
    usecases::{AdapterCatalogReadUseCase, AdapterInspectRequest, StdAdapterCatalogReadUseCase},
};
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::server::{
    domain::{ServerInspection, ServerRefSelector},
    infra::{
        FileServerCatalogStore, StdServerIdentityGenerator, StdServerProcessController,
        StdServerStoreLayoutInitializer, SystemServerClock,
    },
    usecases::{ServerInspectRequest, ServerSpecUseCase, StdServerUseCase},
};
use tentgent_kernel::features::session::{
    domain::{SessionCompactionSummary, SessionRefSelector, SessionStoreConfig},
    infra::{
        FileSessionLockManager, FileSessionStore, StdSessionIdentityGenerator, SystemSessionClock,
    },
    ports::{
        SessionAdapterRefResolutionRequest, SessionAdapterRefResolver, SessionPortFuture,
        SessionServerRefResolutionRequest, SessionServerRefResolver,
        SessionSummaryGenerationRequest, SessionSummaryGenerator,
    },
    usecases::{SessionStoreSelection, StdSessionUseCase},
};
use tentgent_kernel::foundation::{
    error::{KernelError, KernelResult},
    layout::{LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver},
};

pub(super) struct CliSessionKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    session_identity: StdSessionIdentityGenerator,
    session_clock: SystemSessionClock,
    session_locks: FileSessionLockManager,
    session_store: FileSessionStore,
    server_initializer: StdServerStoreLayoutInitializer,
    server_identity: StdServerIdentityGenerator,
    server_catalog: FileServerCatalogStore,
    server_process_controller: StdServerProcessController,
    server_clock: SystemServerClock,
    model_catalog: FileModelCatalogStore,
    adapter_catalog: FileAdapterCatalogStore,
}

impl CliSessionKernel {
    pub(super) fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            session_identity: StdSessionIdentityGenerator,
            session_clock: SystemSessionClock,
            session_locks: FileSessionLockManager::default(),
            session_store: FileSessionStore,
            server_initializer: StdServerStoreLayoutInitializer,
            server_identity: StdServerIdentityGenerator,
            server_catalog: FileServerCatalogStore::default(),
            server_process_controller: StdServerProcessController::default(),
            server_clock: SystemServerClock,
            model_catalog: FileModelCatalogStore,
            adapter_catalog: FileAdapterCatalogStore,
        }
    }

    pub(super) fn session_usecase(&self) -> StdSessionUseCase<'_> {
        StdSessionUseCase::new(
            &self.layout_resolver,
            &self.session_identity,
            &self.session_clock,
            &self.session_locks,
            &self.session_store,
            self,
            self,
            self,
        )
    }

    pub(super) fn inspect_running_server(
        &self,
        home: Option<&Path>,
        reference: &str,
    ) -> KernelResult<ServerInspection> {
        let selector = ServerRefSelector::parse(reference)
            .map_err(|err| session_kernel_error(format!("invalid server reference: {err}")))?;
        let inspected = self.server_usecase().inspect_server(ServerInspectRequest {
            layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
            selector,
        })?;
        if !inspected.inspection.running {
            return Err(session_kernel_error(format!(
                "server `{}` is not running",
                inspected.inspection.spec.short_ref
            )));
        }
        Ok(inspected.inspection)
    }

    fn server_usecase(&self) -> StdServerUseCase<'_> {
        StdServerUseCase::new(
            &self.layout_resolver,
            &self.server_initializer,
            &self.model_catalog,
            &self.server_identity,
            &self.server_catalog,
            &self.server_process_controller,
            &self.server_clock,
        )
    }

    fn adapter_catalog_usecase(&self) -> StdAdapterCatalogReadUseCase<'_> {
        StdAdapterCatalogReadUseCase::new(&self.layout_resolver, &self.adapter_catalog)
    }
}

impl SessionServerRefResolver for CliSessionKernel {
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

impl SessionAdapterRefResolver for CliSessionKernel {
    fn resolve_session_adapter_ref(
        &self,
        request: SessionAdapterRefResolutionRequest,
    ) -> KernelResult<String> {
        let selector = AdapterRefSelector::parse(&request.selector)
            .map_err(|err| session_kernel_error(format!("invalid adapter reference: {err}")))?;
        let inspected = self
            .adapter_catalog_usecase()
            .inspect_adapter(AdapterInspectRequest {
                layout: layout_input_from_session_store(&request.store)?,
                selector,
            })?;
        Ok(inspected.adapter.metadata.adapter_ref.into_string())
    }
}

impl SessionSummaryGenerator for CliSessionKernel {
    fn summarize_session(
        &self,
        _request: SessionSummaryGenerationRequest,
    ) -> SessionPortFuture<'_, SessionCompactionSummary> {
        Box::pin(async {
            Err(session_kernel_error(
                "CLI session summary generation must be handled by the calling command",
            ))
        })
    }
}

pub(super) fn session_store_selection(
    home: Option<&Path>,
    mode: LayoutResolveMode,
) -> SessionStoreSelection {
    SessionStoreSelection::default_file(runtime_layout_input(mode, home))
}

pub(super) fn session_store_selection_from_str(
    home: Option<&str>,
    mode: LayoutResolveMode,
) -> SessionStoreSelection {
    SessionStoreSelection::default_file(RuntimeLayoutInput {
        mode,
        home_dir: home.map(PathBuf::from),
        data_root_dir: None,
    })
}

pub(super) fn parse_session_selector(reference: &str) -> miette::Result<SessionRefSelector> {
    SessionRefSelector::parse(reference)
        .map_err(|err| miette::miette!("invalid session reference: {err}"))
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

fn runtime_layout_input(mode: LayoutResolveMode, home: Option<&Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

fn session_kernel_error(message: impl Into<String>) -> KernelError {
    KernelError::SessionStoreUnavailable(message.into())
}
