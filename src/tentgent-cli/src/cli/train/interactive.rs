use std::io::{self, Write};

use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::train::domain::{
    LoraTrainBackend, LoraTrainOverrides, LoraTrainPlan,
};

pub fn collect_overrides(
    plan: &LoraTrainPlan,
    mut overrides: LoraTrainOverrides,
) -> Result<LoraTrainOverrides> {
    println!("Interactive override editor. Press Enter to keep the shown value.");
    println!();

    overrides.max_seq_length = prompt_u32(
        "max_seq_length",
        plan.dataset.max_seq_length,
        overrides.max_seq_length,
    )?;
    overrides.mask_prompt = prompt_bool(
        "mask_prompt",
        plan.dataset.mask_prompt,
        overrides.mask_prompt,
    )?;
    overrides.rank = prompt_u32("rank", plan.lora.rank, overrides.rank)?;
    overrides.learning_rate = prompt_f64(
        "learning_rate",
        plan.optimization.learning_rate,
        overrides.learning_rate,
    )?;
    overrides.batch_size = prompt_u32(
        "batch_size",
        plan.optimization.batch_size,
        overrides.batch_size,
    )?;
    overrides.gradient_accumulation_steps = prompt_u32(
        "grad_accum",
        plan.optimization.gradient_accumulation_steps,
        overrides.gradient_accumulation_steps,
    )?;
    overrides.max_steps = prompt_u32(
        "max_steps",
        plan.optimization.max_steps,
        overrides.max_steps,
    )?;
    overrides.seed = prompt_u64("seed", plan.optimization.seed, overrides.seed)?;

    match plan.backend {
        Some(LoraTrainBackend::Mlx) => {
            if let Some(mlx) = &plan.backend_config.mlx {
                overrides.mlx_num_layers =
                    prompt_u32("num_layers", mlx.num_layers, overrides.mlx_num_layers)?;
                overrides.mlx_grad_checkpoint = prompt_bool(
                    "grad_checkpoint",
                    mlx.grad_checkpoint,
                    overrides.mlx_grad_checkpoint,
                )?;
            }
        }
        Some(LoraTrainBackend::Peft) => {
            if let Some(peft) = &plan.backend_config.peft {
                overrides.peft_load_in_4bit = prompt_bool(
                    "load_in_4bit",
                    peft.load_in_4bit,
                    overrides.peft_load_in_4bit,
                )?;
                overrides.peft_load_in_8bit = prompt_bool(
                    "load_in_8bit",
                    peft.load_in_8bit,
                    overrides.peft_load_in_8bit,
                )?;
            }
        }
        None => {
            println!("backend-specific prompts skipped because no backend is selected.");
        }
    }

    println!();
    Ok(overrides)
}

pub fn confirm_save_plan() -> Result<bool> {
    loop {
        print!("Save this plan? [Y/n] ");
        io::stdout().flush().into_diagnostic()?;

        let Some(answer) = read_trimmed_line()? else {
            println!();
            return Ok(false);
        };

        match answer.to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please answer y or n."),
        }
    }
}

fn prompt_u32(label: &str, current: u32, existing: Option<u32>) -> Result<Option<u32>> {
    let Some(answer) = prompt(label, &current.to_string())? else {
        return Ok(existing);
    };
    if answer.is_empty() {
        return Ok(existing);
    }

    answer
        .parse::<u32>()
        .map(Some)
        .map_err(|err| miette!("invalid value for `{label}`: {err}"))
}

fn prompt_u64(label: &str, current: u64, existing: Option<u64>) -> Result<Option<u64>> {
    let Some(answer) = prompt(label, &current.to_string())? else {
        return Ok(existing);
    };
    if answer.is_empty() {
        return Ok(existing);
    }

    answer
        .parse::<u64>()
        .map(Some)
        .map_err(|err| miette!("invalid value for `{label}`: {err}"))
}

fn prompt_f64(label: &str, current: f64, existing: Option<f64>) -> Result<Option<f64>> {
    let Some(answer) = prompt(label, &current.to_string())? else {
        return Ok(existing);
    };
    if answer.is_empty() {
        return Ok(existing);
    }

    answer
        .parse::<f64>()
        .map(Some)
        .map_err(|err| miette!("invalid value for `{label}`: {err}"))
}

fn prompt_bool(label: &str, current: bool, existing: Option<bool>) -> Result<Option<bool>> {
    let Some(answer) = prompt(label, if current { "y" } else { "n" })? else {
        return Ok(existing);
    };
    if answer.is_empty() {
        return Ok(existing);
    }

    match answer.to_ascii_lowercase().as_str() {
        "y" | "yes" | "true" | "1" => Ok(Some(true)),
        "n" | "no" | "false" | "0" => Ok(Some(false)),
        _ => Err(miette!("invalid value for `{label}`: expected y or n")),
    }
}

fn prompt(label: &str, current: &str) -> Result<Option<String>> {
    print!("{label} [{current}]: ");
    io::stdout().flush().into_diagnostic()?;
    read_trimmed_line()
}

fn read_trimmed_line() -> Result<Option<String>> {
    let mut input = String::new();
    let bytes_read = io::stdin().read_line(&mut input).into_diagnostic()?;
    if bytes_read == 0 {
        return Ok(None);
    }
    Ok(Some(input.trim().to_string()))
}
