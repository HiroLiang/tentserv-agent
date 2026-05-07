use std::{
    fs,
    process::{Command, Stdio},
};

use console::style;
use miette::{miette, Result};
use tentgent_core::{
    doctor::{build_doctor_report, DoctorCheck, DoctorCheckStatus, DoctorOptions},
    runtime_assets::PythonRuntime,
};

use super::{
    commands::DoctorCommand,
    runtime_footprint::{collect_runtime_footprint_best_effort, FootprintEntry},
};

pub fn handle_doctor_command(command: DoctorCommand) -> Result<()> {
    if command.fix {
        bootstrap_python_env()?;
    }

    let report = build_doctor_report(DoctorOptions::cli());
    render_checks(&report.checks);

    if report.summary.fail > 0 {
        return Err(miette!("doctor found {} failure(s)", report.summary.fail));
    }

    Ok(())
}

fn bootstrap_python_env() -> Result<()> {
    let runtime = PythonRuntime::resolve()
        .map_err(|err| miette!("failed to resolve Python runtime assets: {err}"))?;
    let parent = runtime
        .env_dir()
        .parent()
        .ok_or_else(|| miette!("failed to resolve parent directory for Python env"))?;
    fs::create_dir_all(parent).map_err(|err| {
        miette!(
            "failed to create Python env parent `{}`: {err}",
            parent.display()
        )
    })?;

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Developer Python environment bootstrap").bold()
    );
    println!("project: {}", runtime.project_dir().display());
    println!("env: {}", runtime.env_dir().display());

    let mut process = Command::new("uv");
    process
        .current_dir(runtime.project_dir())
        .env("UV_PROJECT_ENVIRONMENT", runtime.env_dir())
        .arg("--no-config")
        .arg("sync")
        .arg("--project")
        .arg(runtime.project_dir())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = process
        .status()
        .map_err(|err| miette!("failed to run uv for developer Python env bootstrap: {err}"))?;
    if !status.success() {
        return Err(miette!(
            "developer Python env bootstrap failed with status {status}"
        ));
    }

    println!();
    Ok(())
}

fn render_checks(checks: &[DoctorCheck]) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent doctor").bold()
    );

    for check in checks {
        println!(
            "{} {:<34} {}",
            status_marker(check.status),
            check.name,
            short_summary(&check.detail)
        );
    }
    render_details(checks);
    render_runtime_footprint(&collect_runtime_footprint_best_effort());

    let failures = checks
        .iter()
        .filter(|check| check.status == DoctorCheckStatus::Fail)
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.status == DoctorCheckStatus::Warn)
        .count();
    let result = if failures > 0 {
        format!("blocked with {failures} failure(s) and {warnings} warning(s)")
    } else if warnings > 0 {
        format!("ready with {warnings} warning(s)")
    } else {
        "ready".to_string()
    };

    println!("Result: {result}");
    println!();
}

fn render_details(checks: &[DoctorCheck]) {
    let notable = checks
        .iter()
        .filter(|check| check.status != DoctorCheckStatus::Pass || should_show_detail(check))
        .collect::<Vec<_>>();
    if notable.is_empty() {
        return;
    }

    println!();
    println!("{}", style("Details").bold());
    for check in notable {
        println!(
            "{} {}: {}",
            status_marker(check.status),
            style(&check.name).bold(),
            check.detail
        );
    }
}

fn render_runtime_footprint(entries: &[FootprintEntry]) {
    if entries.is_empty() {
        return;
    }

    println!();
    println!("{}", style("Runtime footprint").bold());
    for entry in entries {
        println!(
            "{} {:<34} {}",
            style("info").cyan().bold(),
            entry.title,
            entry.render_value()
        );
        if entry.field == "bootstrap_uv_cache_size" {
            if let Some(guidance) = entry.guidance() {
                println!("   {:<34} {guidance}", "note");
            }
        }
    }
}

fn should_show_detail(check: &DoctorCheck) -> bool {
    matches!(
        check.name.as_str(),
        "runtime home"
            | "bootstrap cache"
            | "python source"
            | "python pyproject"
            | "python env"
            | "python binary"
    )
}

fn short_summary(detail: &str) -> String {
    let summary = detail.split(':').next().unwrap_or(detail).trim();
    truncate_middle(summary, 42)
}

fn truncate_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }

    let head_len = max_chars.saturating_sub(1) / 2;
    let tail_len = max_chars.saturating_sub(1 + head_len);
    let head = value.chars().take(head_len).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}…{tail}")
}

fn status_marker(status: DoctorCheckStatus) -> console::StyledObject<&'static str> {
    match status {
        DoctorCheckStatus::Pass => style("ok").green().bold(),
        DoctorCheckStatus::Warn => style("warn").yellow().bold(),
        DoctorCheckStatus::Fail => style("fail").red().bold(),
        DoctorCheckStatus::Skipped => style("skip").dim(),
    }
}
