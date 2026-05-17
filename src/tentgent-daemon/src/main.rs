use std::path::PathBuf;

use clap::Parser;
use tentgent_daemon::{bootstrap_daemon_app, DaemonBootstrapConfig, LoggingConfig, RestConfig};

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

    #[arg(long, value_name = "FILTER")]
    log_filter: Option<String>,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let args = Args::parse();
    let config = DaemonBootstrapConfig {
        home: args.home,
        logging: LoggingConfig {
            enabled: true,
            env_filter: args.log_filter,
        },
        rest: RestConfig::from_parts(!args.rest_disabled, args.host, args.port),
    };

    bootstrap_daemon_app(config)?.run_until_shutdown().await
}
