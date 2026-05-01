use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, Result};
use tentgent_core::{
    platform::{current_backend_capabilities, PlatformInfo},
    runtime_assets::{resolve_runtime_home, PythonRuntime},
    VERSION,
};

pub fn handle_status_command() -> Result<()> {
    let runtime_home = resolve_runtime_home()
        .map_err(|err| miette!("failed to resolve Tentgent runtime home: {err}"))?;
    let python = PythonRuntime::resolve()
        .map_err(|err| miette!("failed to resolve Python runtime assets: {err}"))?;

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Tentgent status").bold()
    );

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);

    table.add_row(vec![Cell::new("version"), Cell::new(VERSION)]);
    let platform = PlatformInfo::current();
    table.add_row(vec![Cell::new("platform"), Cell::new(platform.label())]);
    table.add_row(vec![
        Cell::new("runtime_home"),
        Cell::new(runtime_home.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("python_source"),
        Cell::new(python.source().as_str()),
    ]);
    table.add_row(vec![
        Cell::new("python_project"),
        Cell::new(path_status(python.project_dir(), "pyproject.toml")),
    ]);
    table.add_row(vec![
        Cell::new("python_env"),
        Cell::new(path_status(python.env_dir(), "")),
    ]);
    table.add_row(vec![
        Cell::new("python_bin"),
        Cell::new(path_status(&python.python_bin(), "")),
    ]);
    table.add_row(vec![
        Cell::new("chat_entrypoint"),
        Cell::new(path_status(&python.script_bin("tentgent-chat-once"), "")),
    ]);
    table.add_row(vec![
        Cell::new("hf_snapshot_entrypoint"),
        Cell::new(path_status(&python.script_bin("tentgent-hf-snapshot"), "")),
    ]);
    for capability in current_backend_capabilities() {
        table.add_row(vec![
            Cell::new(format!("backend_{}", capability.backend.as_str())),
            Cell::new(capability.summary()),
        ]);
    }

    println!("{table}");
    println!();
    Ok(())
}

fn path_status(path: &std::path::Path, required_child: &str) -> String {
    let exists = if required_child.is_empty() {
        path.exists()
    } else {
        path.join(required_child).exists()
    };
    let label = if exists { "present" } else { "missing" };
    format!("{label}: {}", path.display())
}
