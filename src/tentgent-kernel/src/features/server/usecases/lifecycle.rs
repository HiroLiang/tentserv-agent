//! Standard server spec and lifecycle orchestration.

use crate::features::model::ports::ModelCatalogStore;
use crate::features::server::domain::{ServerPrepareOutcome, ServerStopOutcome};
use crate::features::server::ports::{
    ServerCatalogStore, ServerClock, ServerIdentityGenerator, ServerProcessController,
    ServerStoreLayoutInitializer,
};
use crate::foundation::error::{KernelError, KernelResult};
use crate::foundation::layout::RuntimeLayoutResolver;

use super::common::{
    build_server_spec, ensure_server_spec_launchable, resolve_server_runtime_target,
    server_store_layout,
};
use super::port::{
    ServerClearProcessRequest, ServerInspectRequest, ServerInspectResult, ServerLifecycleUseCase,
    ServerListRequest, ServerListResult, ServerPrepareRequest, ServerPrepareResult,
    ServerRecordProcessStartRequest, ServerRemoveRequest, ServerRemoveResult,
    ServerResolveForStartRequest, ServerSpecUseCase, ServerStopRequest, ServerStopResult,
};

/// Standard orchestration for server specs and process metadata.
pub struct StdServerUseCase<'a> {
    layout_resolver: &'a dyn RuntimeLayoutResolver,
    layout_initializer: &'a dyn ServerStoreLayoutInitializer,
    model_catalog: &'a dyn ModelCatalogStore,
    identity: &'a dyn ServerIdentityGenerator,
    catalog: &'a dyn ServerCatalogStore,
    process_controller: &'a dyn ServerProcessController,
    clock: &'a dyn ServerClock,
}

impl<'a> StdServerUseCase<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        layout_resolver: &'a dyn RuntimeLayoutResolver,
        layout_initializer: &'a dyn ServerStoreLayoutInitializer,
        model_catalog: &'a dyn ModelCatalogStore,
        identity: &'a dyn ServerIdentityGenerator,
        catalog: &'a dyn ServerCatalogStore,
        process_controller: &'a dyn ServerProcessController,
        clock: &'a dyn ServerClock,
    ) -> Self {
        Self {
            layout_resolver,
            layout_initializer,
            model_catalog,
            identity,
            catalog,
            process_controller,
            clock,
        }
    }
}

impl ServerSpecUseCase for StdServerUseCase<'_> {
    fn prepare_server(&self, request: ServerPrepareRequest) -> KernelResult<ServerPrepareResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let target = resolve_server_runtime_target(
            &request.runtime_ref,
            request.capability,
            &layout,
            self.model_catalog,
        )?;
        let spec = build_server_spec(
            target,
            request.host.as_deref(),
            request.port,
            request.lazy_load,
            request.idle_seconds,
            self.clock.now_rfc3339()?,
            self.identity,
        )?;
        let selector =
            crate::features::server::domain::ServerRefSelector::parse(spec.server_ref.as_str())
                .map_err(|err| KernelError::ServerStoreUnavailable(err.to_string()))?;

        if store.server_spec_path(spec.server_ref.as_str()).exists() {
            let inspection = self.catalog.inspect_server(&store, &selector)?;
            ensure_server_spec_launchable(&inspection.spec, &layout, self.model_catalog)?;
            return Ok(ServerPrepareResult {
                layout,
                store,
                outcome: ServerPrepareOutcome {
                    inspection,
                    created: false,
                },
            });
        }

        self.layout_initializer.ensure_server_store_layout(&store)?;
        self.catalog.save_server_spec(&store, &spec)?;
        let inspection = self.catalog.inspect_server(&store, &selector)?;

        Ok(ServerPrepareResult {
            layout,
            store,
            outcome: ServerPrepareOutcome {
                inspection,
                created: true,
            },
        })
    }

    fn list_servers(&self, request: ServerListRequest) -> KernelResult<ServerListResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let servers = if request.running_only {
            self.catalog.list_running_servers(&store)?
        } else {
            self.catalog.list_servers(&store)?
        };

        Ok(ServerListResult {
            layout,
            store,
            servers,
        })
    }

    fn inspect_server(&self, request: ServerInspectRequest) -> KernelResult<ServerInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let inspection = self.catalog.inspect_server(&store, &request.selector)?;

        Ok(ServerInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn remove_server(&self, request: ServerRemoveRequest) -> KernelResult<ServerRemoveResult> {
        let inspected = self.inspect_server(ServerInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let outcome = self
            .catalog
            .remove_server(&inspected.store, &inspected.inspection.spec.server_ref)?;

        Ok(ServerRemoveResult {
            layout: inspected.layout,
            store: inspected.store,
            outcome,
        })
    }
}

impl ServerLifecycleUseCase for StdServerUseCase<'_> {
    fn resolve_for_start(
        &self,
        request: ServerResolveForStartRequest,
    ) -> KernelResult<ServerInspectResult> {
        let result = self.inspect_server(ServerInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        ensure_server_spec_launchable(&result.inspection.spec, &result.layout, self.model_catalog)?;
        if result.inspection.running {
            return Err(KernelError::ServerRuntimeUnavailable(format!(
                "server `{}` is already running",
                result.inspection.spec.short_ref
            )));
        }

        Ok(result)
    }

    fn record_process_start(
        &self,
        request: ServerRecordProcessStartRequest,
    ) -> KernelResult<ServerInspectResult> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        let inspection = self.catalog.record_process_start(
            &store,
            &request.server_ref,
            request.pid,
            request.launch_mode,
            self.clock.now_rfc3339()?,
        )?;

        Ok(ServerInspectResult {
            layout,
            store,
            inspection,
        })
    }

    fn clear_process_if_matches(&self, request: ServerClearProcessRequest) -> KernelResult<()> {
        let layout = self.layout_resolver.resolve(request.layout)?;
        let store = server_store_layout(&layout);
        self.catalog
            .clear_process_if_matches(&store, &request.server_ref, request.expected_pid)
    }

    fn stop_server(&self, request: ServerStopRequest) -> KernelResult<ServerStopResult> {
        let inspected = self.inspect_server(ServerInspectRequest {
            layout: request.layout,
            selector: request.selector,
        })?;
        let process = inspected.inspection.process.clone().ok_or_else(|| {
            KernelError::ServerRuntimeUnavailable(format!(
                "server `{}` is not running",
                inspected.inspection.spec.short_ref
            ))
        })?;
        if !inspected.inspection.running {
            return Err(KernelError::ServerRuntimeUnavailable(format!(
                "server `{}` is not running",
                inspected.inspection.spec.short_ref
            )));
        }

        self.process_controller.terminate_process(process.pid)?;
        self.catalog.clear_process_if_matches(
            &inspected.store,
            &inspected.inspection.spec.server_ref,
            Some(process.pid),
        )?;
        let selector = crate::features::server::domain::ServerRefSelector::parse(
            inspected.inspection.spec.server_ref.as_str(),
        )
        .map_err(|err| KernelError::ServerStoreUnavailable(err.to_string()))?;
        let inspection = self.catalog.inspect_server(&inspected.store, &selector)?;

        Ok(ServerStopResult {
            layout: inspected.layout,
            store: inspected.store,
            outcome: ServerStopOutcome {
                inspection,
                stopped_pid: process.pid,
            },
        })
    }
}
