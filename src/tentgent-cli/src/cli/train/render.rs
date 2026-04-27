use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use tentgent_core::train::{
    LoraTrainPlan, LoraTrainPlanCreateOutcome, LoraTrainPlanInspection,
    LoraTrainPlanPreviewOutcome, LoraTrainPlanRemovalOutcome, LoraTrainPlanSummary,
};

pub fn render_plan_create_outcome(outcome: &LoraTrainPlanCreateOutcome) {
    let title = if outcome.deduplicated {
        "LoRA train plan reused"
    } else {
        "LoRA train plan created"
    };
    println!("{} {}", style("==>").cyan().bold(), style(title).bold());
    println!(
        "{} plan {} at {}",
        if outcome.deduplicated {
            style("reused").yellow().bold()
        } else {
            style("stored").green().bold()
        },
        outcome.plan.short_ref,
        outcome.plan_dir.display()
    );
    render_plan_table(&outcome.plan, Some(outcome.run_count));
    println!(
        "{} {}",
        style("Command hint:").bold(),
        outcome.plan.command_hint
    );
    println!();
}

pub fn render_plan_review(outcome: &LoraTrainPlanPreviewOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("LoRA train plan review").bold()
    );
    println!(
        "{} plan {} at {}",
        if outcome.would_reuse {
            style("would reuse").yellow().bold()
        } else {
            style("would store").green().bold()
        },
        outcome.plan.short_ref,
        outcome.plan_dir.display()
    );
    render_plan_table(&outcome.plan, Some(outcome.run_count));
    println!(
        "{} {}",
        style("Command hint:").bold(),
        outcome.plan.command_hint
    );
    println!();
}

pub fn render_plan_inspection(inspection: &LoraTrainPlanInspection) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("LoRA train plan").bold(),
        inspection.plan.short_ref
    );
    render_plan_table(&inspection.plan, Some(inspection.run_count));

    let mut table = base_table();
    table.add_row(vec![
        Cell::new("plan_dir"),
        Cell::new(inspection.plan_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("plan_path"),
        Cell::new(inspection.plan_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("runs_dir"),
        Cell::new(inspection.runs_dir.display().to_string()),
    ]);
    println!("{table}");
    println!(
        "{} {}",
        style("Command hint:").bold(),
        inspection.plan.command_hint
    );
    println!();
}

pub fn render_plan_removal(outcome: &LoraTrainPlanRemovalOutcome) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("LoRA train plan removed").bold()
    );
    println!(
        "{} plan {} from {}",
        style("removed").red().bold(),
        outcome.plan.short_ref,
        outcome.plan_dir.display()
    );
    if outcome.run_count > 0 {
        println!("removed {} run record(s)", outcome.run_count);
    }
    println!();
}

pub fn render_plan_list(plans: &[LoraTrainPlanSummary]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("LoRA train plans").bold()
    );
    if plans.is_empty() {
        println!("no managed LoRA train plans found.");
        println!();
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "short_ref",
            "status",
            "backend",
            "profile",
            "model",
            "dataset",
            "runs",
        ]);

    for summary in plans {
        table.add_row(vec![
            Cell::new(&summary.plan.short_ref),
            Cell::new(summary.plan.status.as_str()),
            Cell::new(
                summary
                    .plan
                    .backend
                    .map(|backend| backend.as_str())
                    .unwrap_or("(none)"),
            ),
            Cell::new(&summary.plan.profile),
            Cell::new(&summary.plan.model_short_ref),
            Cell::new(&summary.plan.dataset_short_ref),
            Cell::new(summary.run_count),
        ]);
    }

    println!("{table}");
    println!();
}

fn render_plan_table(plan: &LoraTrainPlan, run_count: Option<usize>) {
    let mut table = base_table();
    table.add_row(vec![Cell::new("status"), Cell::new(plan.status.as_str())]);
    table.add_row(vec![Cell::new("plan_ref"), Cell::new(&plan.plan_ref)]);
    table.add_row(vec![Cell::new("short_ref"), Cell::new(&plan.short_ref)]);
    if let Some(name) = &plan.name {
        table.add_row(vec![Cell::new("name"), Cell::new(name)]);
    }
    table.add_row(vec![Cell::new("model_ref"), Cell::new(&plan.model_ref)]);
    table.add_row(vec![
        Cell::new("model_format"),
        Cell::new(&plan.model.primary_format),
    ]);
    table.add_row(vec![Cell::new("dataset_ref"), Cell::new(&plan.dataset_ref)]);
    table.add_row(vec![
        Cell::new("train_split"),
        Cell::new(display_option(plan.dataset.train_split.as_deref())),
    ]);
    table.add_row(vec![
        Cell::new("valid_split"),
        Cell::new(display_option(plan.dataset.validation_split.as_deref())),
    ]);
    table.add_row(vec![
        Cell::new("train_examples"),
        Cell::new(display_option_usize(plan.dataset.train_examples)),
    ]);
    table.add_row(vec![
        Cell::new("backend_selected"),
        Cell::new(
            plan.backend
                .map(|backend| backend.as_str())
                .unwrap_or("(none)"),
        ),
    ]);
    table.add_row(vec![Cell::new("profile"), Cell::new(&plan.profile)]);
    table.add_row(vec![
        Cell::new("max_seq_length"),
        Cell::new(plan.dataset.max_seq_length),
    ]);
    table.add_row(vec![
        Cell::new("mask_prompt"),
        Cell::new(display_bool(plan.dataset.mask_prompt)),
    ]);
    table.add_row(vec![Cell::new("lora_rank"), Cell::new(plan.lora.rank)]);
    table.add_row(vec![
        Cell::new("learning_rate"),
        Cell::new(plan.optimization.learning_rate),
    ]);
    table.add_row(vec![
        Cell::new("batch_size"),
        Cell::new(plan.optimization.batch_size),
    ]);
    table.add_row(vec![
        Cell::new("grad_accum"),
        Cell::new(plan.optimization.gradient_accumulation_steps),
    ]);
    table.add_row(vec![
        Cell::new("max_steps"),
        Cell::new(plan.optimization.max_steps),
    ]);
    table.add_row(vec![Cell::new("seed"), Cell::new(plan.optimization.seed)]);
    if let Some(mlx) = &plan.backend_config.mlx {
        table.add_row(vec![Cell::new("mlx_layers"), Cell::new(mlx.num_layers)]);
        table.add_row(vec![
            Cell::new("mlx_grad_ckpt"),
            Cell::new(display_bool(mlx.grad_checkpoint)),
        ]);
    }
    if let Some(peft) = &plan.backend_config.peft {
        table.add_row(vec![
            Cell::new("peft_4bit"),
            Cell::new(display_bool(peft.load_in_4bit)),
        ]);
        table.add_row(vec![
            Cell::new("peft_8bit"),
            Cell::new(display_bool(peft.load_in_8bit)),
        ]);
    }
    if let Some(run_count) = run_count {
        table.add_row(vec![Cell::new("runs"), Cell::new(run_count)]);
    }
    if !plan.blockers.is_empty() {
        table.add_row(vec![
            Cell::new("blockers"),
            Cell::new(plan.blockers.join("\n")),
        ]);
    }
    if !plan.warnings.is_empty() {
        table.add_row(vec![
            Cell::new("warnings"),
            Cell::new(plan.warnings.join("\n")),
        ]);
    }
    println!("{table}");
}

fn base_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);
    table
}

fn display_option(value: Option<&str>) -> String {
    value.unwrap_or("(not set)").to_string()
}

fn display_option_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "(not set)".to_string())
}

fn display_bool(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
