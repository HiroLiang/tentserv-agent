use std::path::PathBuf;

use clap::Parser;
use miette::IntoDiagnostic;
use tentgent_core::daemon::{DaemonManager, DaemonRunRequest};
use tentgent_http::{
    security::{check_bind_safety, DaemonSecurityConfig},
    DaemonHttpServer, DaemonHttpState,
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
    let manager = DaemonManager::new(args.home.as_deref()).into_diagnostic()?;
    let spec = manager
        .prepare_run(DaemonRunRequest {
            host: args.host,
            port: args.port,
        })
        .into_diagnostic()?;
    let security = DaemonSecurityConfig::from_env();
    let bind_safety =
        check_bind_safety(&spec.host, security.token_enabled(), args.allow_unsafe_bind)?;
    for warning in bind_safety.warnings {
        eprintln!("warning: {warning}");
    }
    let server = DaemonHttpServer::bind(spec.host, spec.port).await?;
    let pid = std::process::id();
    let inspection = manager
        .record_process_start(pid, server.host().to_string(), server.port())
        .into_diagnostic()?;

    let process = inspection
        .process
        .as_ref()
        .expect("record_process_start always returns process metadata");
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
    manager
        .clear_process_if_matches(Some(pid))
        .into_diagnostic()?;
    if let Some(result) = serve_result {
        result?;
    }
    println!("tentgent-http stopped");

    Ok(())
}
