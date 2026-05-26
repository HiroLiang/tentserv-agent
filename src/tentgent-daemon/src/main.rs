use std::path::PathBuf;

use clap::Parser;
use tentgent_daemon::{
    bootstrap_daemon_app,
    cloud_server::{run_cloud_server_runtime, CloudServerRuntimeConfig},
    local_server::{run_local_server_runtime, LocalServerRuntimeConfig},
    DaemonBootstrapConfig, LoggingConfig, RestConfig,
};
use tentgent_kernel::features::auth::domain::Provider;
use tentgent_kernel::features::server::domain::ServerCapability;

#[derive(Debug, Parser)]
#[command(name = "tentgent-daemon")]
#[command(about = "Run the Tentgent daemon application host")]
struct Args {
    #[arg(short = 'H', long, value_name = "HOME")]
    home: Option<PathBuf>,

    #[arg(long, value_name = "HOST")]
    host: Option<String>,

    #[arg(long, value_name = "PORT")]
    port: Option<u16>,

    #[arg(long)]
    rest_disabled: bool,

    /// Allow binding to non-loopback or wildcard hosts without a daemon token.
    #[arg(long)]
    allow_unsafe_bind: bool,

    #[arg(long, value_name = "FILTER")]
    log_filter: Option<String>,
}

#[derive(Debug, Parser)]
#[command(name = "__cloud-server-runtime", hide = true)]
struct CloudServerArgs {
    #[arg(long)]
    server_ref: String,
    #[arg(long)]
    provider: String,
    #[arg(long)]
    provider_model: String,
    #[arg(long)]
    host: String,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long)]
    lazy_load: bool,
    #[arg(long = "idle-seconds")]
    idle_seconds: Option<u64>,
}

#[derive(Debug, Parser)]
#[command(name = "__local-server-runtime", hide = true)]
struct LocalServerArgs {
    #[arg(long)]
    server_ref: String,
    #[arg(long)]
    capability: String,
    #[arg(long)]
    model_ref: String,
    #[arg(long)]
    host: String,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long)]
    lazy_load: bool,
    #[arg(long = "idle-seconds")]
    idle_seconds: Option<u64>,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("__cloud-server-runtime") {
        let args = CloudServerArgs::parse_from(
            std::env::args_os()
                .enumerate()
                .filter_map(|(index, value)| (index != 1).then_some(value)),
        );
        let _ = args.lazy_load;
        let provider = match args.provider.trim().to_ascii_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "anthropic" | "claude" => Provider::Anthropic,
            "gemini" | "google" => Provider::Gemini,
            other => return Err(miette::miette!("unsupported cloud provider `{other}`")),
        };
        return run_cloud_server_runtime(CloudServerRuntimeConfig {
            server_ref: args.server_ref,
            provider,
            provider_model: args.provider_model,
            host: args.host,
            port: args.port,
            runtime_home: args.home.map(|path| path.display().to_string()),
        })
        .await;
    }
    if std::env::args().nth(1).as_deref() == Some("__local-server-runtime") {
        let args = LocalServerArgs::parse_from(
            std::env::args_os()
                .enumerate()
                .filter_map(|(index, value)| (index != 1).then_some(value)),
        );
        let _ = args.lazy_load;
        let capability = ServerCapability::parse(&args.capability)
            .map_err(|err| miette::miette!("unsupported local server capability: {err}"))?;
        return run_local_server_runtime(LocalServerRuntimeConfig {
            server_ref: args.server_ref,
            capability,
            model_ref: args.model_ref,
            host: args.host,
            port: args.port,
            runtime_home: args.home,
            idle_seconds: args.idle_seconds,
        })
        .await;
    }
    let args = Args::parse();
    let config = DaemonBootstrapConfig {
        home: args.home,
        logging: LoggingConfig {
            enabled: true,
            env_filter: args.log_filter,
        },
        rest: RestConfig::from_parts(!args.rest_disabled, args.host, args.port)
            .with_allow_unsafe_bind(args.allow_unsafe_bind),
    };

    bootstrap_daemon_app(config)?.run_until_shutdown().await
}
