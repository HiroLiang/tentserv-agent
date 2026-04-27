mod interactive;
mod render;
mod run;
mod run_render;
mod run_summary;

use miette::{IntoDiagnostic, Result};
use tentgent_core::train::LoraTrainPlanManager;

use self::interactive::{collect_overrides, confirm_save_plan};
use self::{
    render::{
        render_plan_create_outcome, render_plan_inspection, render_plan_list, render_plan_removal,
        render_plan_review,
    },
    run::run_lora_plan,
};
use super::commands::{TrainCommands, TrainLoraCommands, TrainLoraPlanCommands};

pub fn handle_train_command(action: TrainCommands) -> Result<()> {
    match action {
        TrainCommands::Lora { action } => handle_lora_command(action)?,
    }

    Ok(())
}

fn handle_lora_command(action: TrainLoraCommands) -> Result<()> {
    match action {
        TrainLoraCommands::Plan { action } => handle_lora_plan_command(action)?,
        TrainLoraCommands::Run(command) => run_lora_plan(command)?,
    }

    Ok(())
}

fn handle_lora_plan_command(action: TrainLoraPlanCommands) -> Result<()> {
    match action {
        TrainLoraPlanCommands::Create(command) => {
            let manager = LoraTrainPlanManager::new().into_diagnostic()?;
            let backend = command.backend.into();
            let name = command.name.clone();
            let mut overrides = command.overrides();

            if command.interactive {
                let preview = manager
                    .preview_plan(
                        &command.model,
                        &command.dataset,
                        backend,
                        name.clone(),
                        overrides.clone(),
                    )
                    .into_diagnostic()?;
                render_plan_review(&preview);
                overrides = collect_overrides(&preview.plan, overrides)?;
            }

            if command.review || command.interactive {
                let preview = manager
                    .preview_plan(
                        &command.model,
                        &command.dataset,
                        backend,
                        name.clone(),
                        overrides.clone(),
                    )
                    .into_diagnostic()?;
                render_plan_review(&preview);

                if !confirm_save_plan()? {
                    println!("plan not saved.");
                    println!();
                    return Ok(());
                }
            }

            let outcome = manager
                .create_plan(&command.model, &command.dataset, backend, name, overrides)
                .into_diagnostic()?;
            render_plan_create_outcome(&outcome);
        }
        TrainLoraPlanCommands::Ls => {
            let manager = LoraTrainPlanManager::new().into_diagnostic()?;
            let plans = manager.list_plans().into_diagnostic()?;
            render_plan_list(&plans);
        }
        TrainLoraPlanCommands::Inspect { reference } => {
            let manager = LoraTrainPlanManager::new().into_diagnostic()?;
            let inspection = manager.inspect_plan(&reference).into_diagnostic()?;
            render_plan_inspection(&inspection);
        }
        TrainLoraPlanCommands::Rm { reference } => {
            let manager = LoraTrainPlanManager::new().into_diagnostic()?;
            let outcome = manager.remove_plan(&reference).into_diagnostic()?;
            render_plan_removal(&outcome);
        }
    }

    Ok(())
}
