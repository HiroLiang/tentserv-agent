use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{IntoDiagnostic, Result};
use tentgent_kernel::features::store::domain::StoreStagingGarbageItem;
use tentgent_kernel::features::store::infra::FileStoreGarbageCollector;
use tentgent_kernel::features::store::usecases::{
    StdStoreGcUseCase, StoreGcRequest, StoreGcUseCase,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::{StoreCommands, StoreGcCommand};
use super::display::format_bytes;

pub fn handle_store_command(action: StoreCommands) -> Result<()> {
    match action {
        StoreCommands::Gc(command) => handle_gc(command),
    }
}

fn handle_gc(command: StoreGcCommand) -> Result<()> {
    let layout_resolver = StdRuntimeLayoutResolver;
    let garbage_collector = FileStoreGarbageCollector;
    let usecase = StdStoreGcUseCase::new(&layout_resolver, &garbage_collector);
    let result = usecase
        .gc_stores(StoreGcRequest {
            layout: RuntimeLayoutInput {
                mode: LayoutResolveMode::ReadOnly,
                home_dir: command.home,
                data_root_dir: None,
            },
            apply: command.apply,
        })
        .into_diagnostic()?;
    let outcome = result.outcome;

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Managed store garbage collection").bold()
    );
    println!("home: {}", result.layout.home_dir.display());
    println!("mode: {}", if outcome.apply { "apply" } else { "dry-run" });

    if outcome.items.is_empty() {
        println!("no abandoned staging directories found");
        return Ok(());
    }

    render_candidates(&outcome.items);

    if outcome.apply {
        println!(
            "removed {} staging director{} ({})",
            outcome.removed_count,
            if outcome.removed_count == 1 {
                "y"
            } else {
                "ies"
            },
            format_bytes(outcome.total_bytes)
        );
    } else {
        println!(
            "dry run only; add --apply to remove {} staging director{} ({})",
            outcome.items.len(),
            if outcome.items.len() == 1 { "y" } else { "ies" },
            format_bytes(outcome.total_bytes)
        );
    }

    Ok(())
}

fn render_candidates(candidates: &[StoreStagingGarbageItem]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS);
    table.set_header(vec![
        Cell::new("store"),
        Cell::new("size"),
        Cell::new("staging path"),
    ]);
    for candidate in candidates {
        table.add_row(vec![
            Cell::new(candidate.store.as_str()),
            Cell::new(format_bytes(candidate.bytes)),
            Cell::new(candidate.path.display().to_string()),
        ]);
    }
    println!("{table}");
}
