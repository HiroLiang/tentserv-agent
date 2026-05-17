use std::path::PathBuf;

use clap::Parser;
use miette::IntoDiagnostic;
use tentgent_http::{security::DaemonSecurityConfig, DaemonHttpServer, DaemonHttpState};
use tentgent_kernel::{
    features::daemon::{
        domain::DaemonBind,
        infra::{
            daemon_runtime_layout_input as daemon_layout, StdDaemonKernel,
            DEFAULT_DAEMON_PROBE_TIMEOUT,
        },
        usecases::{
            DaemonClearProcessRequest, DaemonLifecycleUseCase, DaemonPrepareRunRequest,
            DaemonRecordProcessStartRequest,
        },
    },
    foundation::layout::LayoutResolveMode,
};

#[derive(Debug, Parser)]
#[command(
    name = "tentgent-http",
    version,
    about = "Low-level Tentgent HTTP daemon entry point.",
    long_about = "Low-level Tentgent HTTP daemon entry point. It starts the local Rust HTTP daemon and serves lifecycle endpoints for integrations. Loopback binds can run without auth for development. Non-loopback or wildcard binds require `TENTGENT_DAEMON_TOKEN` unless `--allow-unsafe-bind` is passed."
)]
struct Args {
    /// Optional Tentgent runtime home override for daemon state.
    #[arg(short = 'H', long, value_name = "HOME")]
    home: Option<PathBuf>,
    /// Host interface for the future HTTP listener.
    #[arg(short = 'a', long, value_name = "HOST")]
    host: Option<String>,
    /// TCP port for the future HTTP listener.
    #[arg(short = 'p', long, value_name = "PORT")]
    port: Option<u16>,
    /// Allow binding to non-loopback or wildcard hosts without a daemon token.
    #[arg(long)]
    allow_unsafe_bind: bool,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();
    let kernel = StdDaemonKernel::new(DEFAULT_DAEMON_PROBE_TIMEOUT).into_diagnostic()?;
    let daemon = kernel.usecase();
    let security = DaemonSecurityConfig::from_env();
    let prepared = daemon
        .prepare_run(DaemonPrepareRunRequest {
            layout: daemon_layout(args.home.clone(), LayoutResolveMode::Create),
            host: args.host,
            port: args.port,
            token_enabled: security.token_enabled(),
            allow_unsafe_bind: args.allow_unsafe_bind,
        })
        .into_diagnostic()?;
    for warning in prepared.bind_warnings {
        eprintln!("warning: {warning}");
    }
    let server = DaemonHttpServer::bind(prepared.bind.host.clone(), prepared.bind.port).await?;
    let pid = std::process::id();
    let recorded = daemon
        .record_process_start(DaemonRecordProcessStartRequest {
            layout: daemon_layout(
                Some(prepared.layout.home_dir.clone()),
                LayoutResolveMode::ReadOnly,
            ),
            pid,
            bind: DaemonBind {
                host: server.host().to_string(),
                port: server.port(),
            },
        })
        .into_diagnostic()?;
    let inspection = recorded.inspection;

    let process = inspection
        .process
        .as_ref()
        .expect("record_process_start always returns process metadata");
    let home_dir = inspection.home_dir.clone();
    println!(
        "tentgent-http listening on {}:{} as pid {}",
        process.host, process.port, process.pid
    );
    println!("try GET /healthz or GET /v1/status; press Ctrl-C to stop.");

    let serve_result = tokio::select! {
        result = server.serve(DaemonHttpState::with_security(inspection, security)) => Some(result),
        signal = tokio::signal::ctrl_c() => {
            signal.into_diagnostic()?;
            None
        }
    };
    daemon
        .clear_process_if_matches(DaemonClearProcessRequest {
            layout: daemon_layout(Some(home_dir), LayoutResolveMode::ReadOnly),
            expected_pid: Some(pid),
        })
        .into_diagnostic()?;
    if let Some(result) = serve_result {
        result?;
    }
    println!("tentgent-http stopped");

    Ok(())
}
