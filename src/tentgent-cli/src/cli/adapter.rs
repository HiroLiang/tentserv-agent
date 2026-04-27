use clap::CommandFactory;
use miette::{miette, IntoDiagnostic, Result};
use tentgent_core::adapter::{AdapterError, AdapterManager};

use super::adapter_list::render_adapter_list;
use super::adapter_progress::PullProgress;
use super::adapter_render::{
    render_adapter_inspection, render_bind_outcome, render_import_outcome, render_removal_outcome,
};
use super::app::Cli;
use super::commands::AdapterCommands;

pub fn handle_adapter_command(action: AdapterCommands) -> Result<()> {
    match action {
        AdapterCommands::Add {
            path,
            base_model_ref,
        } => {
            let manager = AdapterManager::new().into_diagnostic()?;
            let outcome = manager
                .add_path(path, base_model_ref.as_deref())
                .into_diagnostic()?;
            render_import_outcome("Adapter imported", &outcome);
        }
        AdapterCommands::Pull {
            repo_id,
            revision,
            base_model_ref,
        } => {
            if is_help_token(&repo_id) {
                print_adapter_subcommand_help("pull")?;
                return Ok(());
            }

            let manager = AdapterManager::new().into_diagnostic()?;
            let mut progress = PullProgress::new(&repo_id, revision.as_deref());
            let outcome = manager.pull_hf_with_progress(
                &repo_id,
                revision.as_deref(),
                base_model_ref.as_deref(),
                |event| {
                    progress.update(event);
                },
            );
            progress.finish();

            let outcome = outcome.into_diagnostic()?;
            render_import_outcome("Adapter pulled", &outcome);
        }
        AdapterCommands::Ls => {
            let manager = AdapterManager::new().into_diagnostic()?;
            let adapters = manager.list_adapters().into_diagnostic()?;
            render_adapter_list(&adapters);
        }
        AdapterCommands::Inspect { reference } => {
            if is_help_token(&reference) {
                print_adapter_subcommand_help("inspect")?;
                return Ok(());
            }

            let manager = AdapterManager::new().into_diagnostic()?;
            let inspection = match manager.inspect(&reference) {
                Ok(inspection) => inspection,
                Err(err) => {
                    return Err(explain_adapter_lookup_error("inspect", "ADAPTER_REF", err))
                }
            };
            render_adapter_inspection(&inspection);
        }
        AdapterCommands::Bind {
            adapter_ref,
            base_model_ref,
        } => {
            if is_help_token(&adapter_ref) {
                print_adapter_subcommand_help("bind")?;
                return Ok(());
            }

            let Some(base_model_ref) = base_model_ref else {
                return Err(miette!(
                    "missing required option `--base-model-ref <MODEL_REF>`\n\nUsage: tentgent adapter bind <ADAPTER_REF> --base-model-ref <MODEL_REF>"
                ));
            };

            let manager = AdapterManager::new().into_diagnostic()?;
            let outcome = match manager.bind_to_model(&adapter_ref, &base_model_ref) {
                Ok(outcome) => outcome,
                Err(err) => return Err(explain_adapter_lookup_error("bind", "ADAPTER_REF", err)),
            };
            render_bind_outcome(&outcome);
        }
        AdapterCommands::Rm { reference } => {
            if is_help_token(&reference) {
                print_adapter_subcommand_help("rm")?;
                return Ok(());
            }

            let manager = AdapterManager::new().into_diagnostic()?;
            let outcome = match manager.remove(&reference) {
                Ok(outcome) => outcome,
                Err(err) => return Err(explain_adapter_lookup_error("rm", "ADAPTER_REF", err)),
            };
            render_removal_outcome(&outcome);
        }
    }

    Ok(())
}

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn print_adapter_subcommand_help(name: &str) -> Result<()> {
    let mut root = Cli::command();
    let adapter = root
        .find_subcommand_mut("adapter")
        .ok_or_else(|| miette!("adapter command metadata is unavailable"))?;
    let subcommand = adapter
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("adapter subcommand `{name}` is unavailable"))?;
    subcommand.print_help().into_diagnostic()?;
    println!();
    Ok(())
}

fn explain_adapter_lookup_error(
    command: &str,
    value_name: &str,
    err: AdapterError,
) -> miette::Report {
    match err {
        AdapterError::NotFound(_) | AdapterError::AmbiguousRef(_) => {
            let usage = match command {
                "bind" => {
                    format!("tentgent adapter bind <{value_name}> --base-model-ref <MODEL_REF>")
                }
                "rm" => format!("tentgent adapter rm <{value_name}>"),
                _ => format!("tentgent adapter {command} <{value_name}>"),
            };
            miette!(
                "{err}\n\nUsage: {usage}\nHint: use `tentgent adapter {command} --help` for the command template."
            )
        }
        other => miette!("{other}"),
    }
}
