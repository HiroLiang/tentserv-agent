use std::{net::SocketAddr, time::Duration};

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
    leases::RouteGenerationManager,
    lifecycle::serve_cluster,
    startup::{prepare_cluster_startup, ClusterStartupState},
    state::{ClusterServerRuntimeConfig, ClusterServerState},
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
        startup: ClusterStartupState::new(
            definitions
                .current()
                .map_err(|err| miette::miette!("{err}"))?,
            config.load_mode,
        ),
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
    // Binding must succeed before any route claim or model load is started.
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| miette::miette!("cluster server proxy bind failed: {err}"))?;
    serve_cluster(
        listener,
        &state,
        prepare_cluster_startup(&state),
        wait_for_shutdown_signal(),
        Duration::from_secs(30),
    )
    .await
    .map_err(|err| miette::miette!("{err}"))
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
