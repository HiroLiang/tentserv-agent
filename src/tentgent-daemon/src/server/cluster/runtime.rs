use std::{future::IntoFuture, net::SocketAddr};

use tentgent_kernel::{
    features::runtime::{
        domain::PythonRuntimeResolutionInput,
        infra::{StdPythonRuntimeResolver, StdRuntimeExecutableResolver},
        ports::PythonRuntimeResolver,
    },
    foundation::layout::{
        LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
    },
};

use super::{
    cache::ClusterDefinitionCache,
    error::ClusterServerError,
    leases::RouteGenerationManager,
    router::cluster_router,
    state::{ClusterServerRuntimeConfig, ClusterServerState},
    watch,
};

pub async fn run_cluster_server_runtime(config: ClusterServerRuntimeConfig) -> miette::Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|err| miette::miette!("invalid cluster server bind address: {err}"))?;
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: config.runtime_home.clone(),
            data_root_dir: None,
        })
        .map_err(|err| miette::miette!("{err}"))?;
    let runtime = StdPythonRuntimeResolver
        .resolve_python_runtime(&layout, PythonRuntimeResolutionInput::default())
        .map_err(|err| miette::miette!("{err}"))?;
    let definitions = ClusterDefinitionCache::load(&layout, config.cluster_ref.clone())
        .map_err(|err| miette::miette!("{err}"))?;
    let routes = RouteGenerationManager::new(
        layout.clone(),
        config.server_ref.clone(),
        config.cluster_ref.clone(),
    );
    let state = ClusterServerState {
        launch_policy:
            tentgent_kernel::features::runtime::infra::ModelRuntimeDaemonLaunchPolicy::new(
                config.runtime_idle_seconds,
                config.model_idle_seconds,
            )
            .map_err(|error| miette::miette!(error))?,
        config,
        layout,
        runtime,
        executable_resolver: StdRuntimeExecutableResolver,
        supervisor: tentgent_kernel::features::runtime::infra::ModelRuntimeDaemonSupervisor::new(),
        client: reqwest::Client::new(),
        definitions,
        routes,
    };
    let (watch_cancel_tx, watch_cancel_rx) = tokio::sync::watch::channel(false);
    let watcher = tokio::spawn(watch::run_definition_watcher(
        state.definitions.clone(),
        state.routes.clone(),
        watch_cancel_rx,
    ));
    let shutdown_state = state.clone();
    let router = cluster_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("cluster server proxy bind failed: {err}"))?;
    let (graceful_tx, graceful_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = graceful_rx.await;
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => {
            result.map_err(|err| miette::miette!("cluster server proxy failed: {err}"))?;
        }
        _ = wait_for_shutdown_signal() => {
            let _ = watch_cancel_tx.send(true);
            shutdown_state.routes.begin_drain();
            let _ = graceful_tx.send(());
            let shutdown = async {
                let (server_result, drain_result) = tokio::join!(
                    &mut server,
                    shutdown_state.routes.finish_drain(),
                );
                drain_result?;
                server_result.map_err(|error| ClusterServerError::route_unavailable(error.to_string()))
            };
            match tokio::time::timeout(std::time::Duration::from_secs(30), shutdown).await {
                Ok(result) => {
                    result.map_err(|err| miette::miette!("cluster server proxy failed during graceful shutdown: {err}"))?;
                }
                Err(_) => shutdown_state.routes.preserve_on_timeout(),
            }
        }
    }
    let _ = watch_cancel_tx.send(true);
    let _ = watcher.await;
    Ok(())
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
        match terminate {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
