use console::style;
use indicatif::ProgressBar;
use serde_json::Value;

pub fn render_event(event: &Value, verbose: bool, debug: bool, progress: &mut Option<ProgressBar>) {
    match event.get("type").and_then(Value::as_str) {
        Some("stage") => render_with_progress_suspended(progress, || render_stage(event, verbose)),
        Some("params") => render_with_progress_suspended(progress, || render_params(event)),
        Some("dataset") => render_with_progress_suspended(progress, || render_dataset(event)),
        Some("train") => render_train(event, progress),
        Some("eval") if verbose => {
            render_with_progress_suspended(progress, || render_eval(event));
        }
        Some("checkpoint") if verbose => {
            render_with_progress_suspended(progress, || render_checkpoint(event));
        }
        Some("done") => render_with_progress_suspended(progress, || render_done(event)),
        Some("error") => render_with_progress_suspended(progress, || render_error(event)),
        Some(_) if debug => render_with_progress_suspended(progress, || {
            println!("{} event {}", style("•").yellow().bold(), event)
        }),
        _ => {}
    }
}

fn render_with_progress_suspended(progress: &Option<ProgressBar>, render: impl FnOnce()) {
    if let Some(progress) = progress {
        progress.suspend(render);
    } else {
        render();
    }
}

fn render_stage(event: &Value, verbose: bool) {
    let status = event
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("done");
    if status != "completed" && !verbose {
        return;
    }
    let name = event.get("name").and_then(Value::as_str).unwrap_or("stage");
    let marker = if status == "completed" { "✓" } else { "•" };
    let label = name.replace('_', " ");
    if status == "completed" {
        println!("{} {}", style(marker).green().bold(), label);
    } else {
        println!("{} {} {}", style(marker).green().bold(), label, status);
    }
}

fn render_dataset(event: &Value) {
    let train = event
        .get("train_examples")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let validation = event
        .get("validation_examples")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let train_tokens = event
        .get("train_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let truncated = event
        .get("truncated_examples")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    println!(
        "{} tokenized dataset train {train} examples, validation {validation}, {train_tokens} train tokens, {truncated} truncated",
        style("✓").green().bold(),
    );
}

fn render_params(event: &Value) {
    let trainable = event.get("trainable").and_then(Value::as_u64).unwrap_or(0);
    let total = event.get("total").and_then(Value::as_u64).unwrap_or(0);
    let percent = event.get("percent").and_then(Value::as_f64).unwrap_or(0.0);
    println!(
        "{} trainable parameters {} / {} ({percent:.3}%)",
        style("✓").green().bold(),
        trainable,
        total
    );
}

fn render_train(event: &Value, progress: &mut Option<ProgressBar>) {
    let step = event.get("step").and_then(Value::as_u64).unwrap_or(0);
    let max_steps = event
        .get("max_steps")
        .and_then(Value::as_u64)
        .unwrap_or(step);
    let loss = event.get("loss").and_then(Value::as_f64).unwrap_or(0.0);
    let lr = event
        .get("learning_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let tokens = event
        .get("tokens_per_sec")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let memory = event
        .get("peak_memory_gb")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let progress = progress.get_or_insert_with(|| ProgressBar::new(max_steps));
    progress.set_length(max_steps);
    progress.set_position(step);
    progress.set_message(format!(
        "loss {loss:.3} lr {lr:.3e} {tokens:.1} tok/s peak {memory:.2} GB"
    ));
}

fn render_eval(event: &Value) {
    let step = event.get("step").and_then(Value::as_u64).unwrap_or(0);
    let loss = event.get("loss").and_then(Value::as_f64).unwrap_or(0.0);
    println!(
        "{} eval step {step} loss {loss:.3}",
        style("✓").green().bold()
    );
}

fn render_checkpoint(event: &Value) {
    let step = event.get("step").and_then(Value::as_u64).unwrap_or(0);
    let path = event.get("path").and_then(Value::as_str).unwrap_or("-");
    println!(
        "{} checkpoint step {step} {}",
        style("✓").green().bold(),
        path
    );
}

fn render_done(event: &Value) {
    if let Some(adapter_path) = event.get("adapter_path").and_then(Value::as_str) {
        println!(
            "{} runner produced adapter output {}",
            style("✓").green().bold(),
            adapter_path
        );
    }
}

fn render_error(event: &Value) {
    let message = event
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("training backend reported an error");
    println!("{} {message}", style("×").red().bold());
}
