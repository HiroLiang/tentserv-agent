use super::*;

pub(super) fn server_model_support_lines(
    kernel: &CliServerKernel,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
) -> Option<String> {
    let model_ref = inspection.spec.local_model_ref()?;
    let capability = inspection.spec.capability?;
    let selector = match ModelRefSelector::parse(model_ref.as_str()) {
        Ok(selector) => selector,
        Err(err) => {
            return Some(format!(
                "status: unavailable\nreason: invalid bound model_ref: {err}"
            ));
        }
    };

    let catalog = StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model = match catalog.inspect_model(ModelInspectRequest {
        layout: runtime_layout_input_from_layout(layout, LayoutResolveMode::ReadOnly),
        selector: selector.clone(),
    }) {
        Ok(result) => result.model,
        Err(err) => {
            return Some(format!(
                "capability: {}\nstatus: unavailable\nreason: model lookup failed: {err}",
                capability.required_model_capability().as_str()
            ));
        }
    };

    let proofs = match kernel
        .model_capability_proof_usecase()
        .list_model_capability_proofs(ModelCapabilityProofListRequest {
            layout: runtime_layout_input_from_layout(layout, LayoutResolveMode::ReadOnly),
            selector,
        }) {
        Ok(result) => result.proofs,
        Err(err) => {
            return Some(format!(
                "capability: {}\nstatus: unavailable\nreason: proof lookup failed: {err}",
                capability.required_model_capability().as_str()
            ));
        }
    };

    let required_capability = capability.required_model_capability();
    let runtime_profile = inspection
        .spec
        .runtime_profile
        .as_ref()
        .map(|profile| (profile.profile_id.as_str(), profile.profile_version));
    let summaries =
        model_support_summaries_with_runtime_profile(&model.metadata, &proofs, runtime_profile);
    summaries
        .into_iter()
        .find(|summary| summary.capability == required_capability)
        .map(|summary| {
            model_support_diagnostic_lines(&summary, Some(&model.metadata.short_ref)).join("\n")
        })
        .or_else(|| {
            Some(format!(
                "capability: {}\nstatus: unknown\nreason: no support summary is available for the bound model",
                required_capability.as_str()
            ))
        })
}

pub(super) fn runtime_layout_input(
    mode: LayoutResolveMode,
    home: Option<&Path>,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

pub(super) fn runtime_layout_input_from_layout(
    layout: &RuntimeLayout,
    mode: LayoutResolveMode,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: Some(layout.home_dir.clone()),
        data_root_dir: Some(layout.data_root_dir.clone()),
    }
}

pub(super) fn render_server_spec_outcome(
    outcome: &tentgent_kernel::features::server::domain::ServerPrepareOutcome,
    detached: bool,
) {
    let inspection = &outcome.inspection;
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style(if outcome.created {
            "Server spec created"
        } else {
            "Server spec reused"
        })
        .bold()
    );
    println!(
        "{} server {} at {}",
        if outcome.created {
            style("stored").green().bold()
        } else {
            style("reused").yellow().bold()
        },
        inspection.spec.short_ref,
        inspection.spec_path.display()
    );
    if inspection.spec.is_cloud() {
        println!(
            "{} cloud provider auth will be verified before runtime launch.",
            style("checking").yellow().bold()
        );
    } else if inspection.spec.is_cluster() {
        println!(
            "{} the cluster server proxy in {} mode.",
            style("starting").green().bold(),
            if detached { "background" } else { "foreground" }
        );
    } else {
        println!(
            "{} the local server proxy in {} mode.",
            style("starting").green().bold(),
            if detached { "background" } else { "foreground" }
        );
    }

    println!("{}", render_server_table(inspection));
    println!();
}

pub(super) fn render_server_list(title: &str, servers: &[ServerSummary]) {
    println!("{} {}", style("==>").cyan().bold(), style(title).bold());

    if servers.is_empty() {
        println!(
            "{} No matching servers were found.\n",
            style("empty").yellow().bold()
        );
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            "short_ref",
            "status",
            "mode",
            "runtime",
            "capability",
            "provider",
            "target",
            "host",
            "port",
            "requested",
            "pid",
        ]);

    for server in servers {
        let mode = if server.running {
            server
                .process
                .as_ref()
                .map(|process| process.launch_mode.as_str())
                .unwrap_or("-")
        } else {
            "-"
        };
        let pid = if server.running {
            server
                .process
                .as_ref()
                .map(|process| process.pid.to_string())
                .unwrap_or_else(|| "-".to_string())
        } else {
            "-".to_string()
        };

        table.add_row(vec![
            Cell::new(&server.spec.short_ref),
            Cell::new(if server.running { "running" } else { "stopped" }),
            Cell::new(mode),
            Cell::new(server.spec.runtime_kind.as_str()),
            Cell::new(
                server
                    .spec
                    .capability
                    .map(ServerCapability::as_str)
                    .unwrap_or("multi-route"),
            ),
            Cell::new(server.spec.provider_label()),
            Cell::new(server_list_target_label(&server.spec)),
            Cell::new(&server.spec.host),
            Cell::new(server.effective_port()),
            Cell::new(server_requested_port_label(&server.spec)),
            Cell::new(pid),
        ]);
    }

    println!("{table}");
    println!();
}

pub(super) fn server_list_target_label(spec: &ServerSpec) -> String {
    match spec.runtime_kind {
        ServerRuntimeKind::Local => spec
            .local_model_ref()
            .map(|model_ref| model_ref.short_ref().to_string())
            .unwrap_or_else(|| "(missing)".to_string()),
        ServerRuntimeKind::Cloud => spec.runtime_model_label(),
        ServerRuntimeKind::Cluster => spec.runtime_model_label(),
    }
}

pub(super) fn render_server_inspection(
    title: &str,
    inspection: &ServerInspection,
    model_support: Option<&str>,
) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style(title).bold(),
        style(&inspection.spec.short_ref).bold()
    );
    println!(
        "{}",
        render_server_table_with_model_support(inspection, model_support)
    );
    println!();
}

pub(super) fn render_server_started(inspection: &ServerInspection, details: bool) {
    println!(
        "{} {} {}",
        style("==>").cyan().bold(),
        style("Server started").bold(),
        inspection.spec.short_ref
    );
    let pid = inspection
        .process
        .as_ref()
        .map(|process| process.pid.to_string())
        .unwrap_or_else(|| "(unknown)".to_string());
    println!(
        "{} server {} pid {}",
        style("started").green().bold(),
        inspection.spec.short_ref,
        pid
    );
    if details {
        println!("{}", render_server_table(inspection));
        println!();
    }
}

pub(super) fn render_server_stop(outcome: &ServerStopOutcome, details: bool) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Server stopped").bold()
    );
    println!(
        "{} server {} pid {}",
        style("stopped").red().bold(),
        outcome.inspection.spec.short_ref,
        outcome.stopped_pid
    );
    if details {
        println!("{}", render_server_table(&outcome.inspection));
        println!();
    }
}

pub(super) fn render_server_removed(inspection: &ServerInspection, details: bool) {
    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("Server removed").bold()
    );
    println!(
        "{} server {} from {}",
        style("removed").red().bold(),
        inspection.spec.short_ref,
        inspection.server_dir.display()
    );
    if details {
        println!("{}", render_server_table(inspection));
        println!();
    }
}

pub(super) fn render_server_table(inspection: &ServerInspection) -> Table {
    render_server_table_with_model_support(inspection, None)
}

pub(super) fn render_server_table_with_model_support(
    inspection: &ServerInspection,
    model_support: Option<&str>,
) -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Field", "Value"]);

    table.add_row(vec![
        Cell::new("server_ref"),
        Cell::new(inspection.spec.server_ref.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("short_ref"),
        Cell::new(&inspection.spec.short_ref),
    ]);
    table.add_row(vec![
        Cell::new("runtime"),
        Cell::new(inspection.spec.runtime_kind.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("capability"),
        Cell::new(
            inspection
                .spec
                .capability
                .map(ServerCapability::as_str)
                .unwrap_or("multi-route"),
        ),
    ]);
    if inspection.spec.is_cloud() {
        table.add_row(vec![
            Cell::new("provider"),
            Cell::new(inspection.spec.provider_label()),
        ]);
        table.add_row(vec![
            Cell::new("provider_model"),
            Cell::new(inspection.spec.runtime_model_label()),
        ]);
    } else if inspection.spec.is_cluster() {
        table.add_row(vec![
            Cell::new("cluster_ref"),
            Cell::new(inspection.spec.runtime_model_label()),
        ]);
    } else {
        table.add_row(vec![
            Cell::new("model_ref"),
            Cell::new(inspection.spec.runtime_model_label()),
        ]);
        if let Some(runtime_profile) = inspection.spec.runtime_profile.as_ref() {
            table.add_row(vec![
                Cell::new("runtime_profile"),
                Cell::new(runtime_profile.label()),
            ]);
            table.add_row(vec![
                Cell::new("runtime_profile_version"),
                Cell::new(runtime_profile.profile_version.to_string()),
            ]);
        }
        if let Some(model_support) = model_support {
            table.add_row(vec![Cell::new("model_support"), Cell::new(model_support)]);
        }
    }
    table.add_row(vec![
        Cell::new("status"),
        Cell::new(if inspection.running {
            "running"
        } else {
            "stopped"
        }),
    ]);
    table.add_row(vec![
        Cell::new("home"),
        Cell::new(inspection.home_dir.display().to_string()),
    ]);
    table.add_row(vec![Cell::new("host"), Cell::new(&inspection.spec.host)]);
    table.add_row(vec![
        Cell::new("port"),
        Cell::new(inspection.effective_port()),
    ]);
    table.add_row(vec![
        Cell::new("requested_port"),
        Cell::new(server_requested_port_label(&inspection.spec)),
    ]);
    table.add_row(vec![
        Cell::new("bound_port"),
        Cell::new(
            inspection
                .bound_port()
                .map(|port| port.to_string())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("lazy_load"),
        Cell::new(if inspection.spec.lazy_load {
            "true"
        } else {
            "false"
        }),
    ]);
    if inspection.spec.is_cloud() {
        table.add_row(vec![
            Cell::new("idle_seconds"),
            Cell::new(
                inspection
                    .spec
                    .idle_seconds
                    .map(|seconds| seconds.to_string())
                    .unwrap_or_else(|| "(not set)".to_string()),
            ),
        ]);
    } else if let Ok(policy) = inspection.spec.model_runtime_idle_policy() {
        table.add_row(vec![
            Cell::new("runtime_idle_seconds"),
            Cell::new(policy.runtime_idle_seconds),
        ]);
        table.add_row(vec![
            Cell::new("model_idle_seconds"),
            Cell::new(policy.model_idle_seconds),
        ]);
    }
    table.add_row(vec![
        Cell::new("created_at"),
        Cell::new(&inspection.spec.created_at),
    ]);
    table.add_row(vec![
        Cell::new("launch_mode"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.launch_mode.as_str().to_string())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("pid"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.pid.to_string())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("started_at"),
        Cell::new(
            inspection
                .process
                .as_ref()
                .map(|process| process.started_at.clone())
                .unwrap_or_else(|| "(not running)".to_string()),
        ),
    ]);
    table.add_row(vec![
        Cell::new("server_dir"),
        Cell::new(inspection.server_dir.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("spec_path"),
        Cell::new(inspection.spec_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("process_path"),
        Cell::new(inspection.process_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("stdout_log"),
        Cell::new(inspection.stdout_log_path.display().to_string()),
    ]);
    table.add_row(vec![
        Cell::new("stderr_log"),
        Cell::new(inspection.stderr_log_path.display().to_string()),
    ]);

    table
}

pub(super) fn server_requested_port_label(spec: &ServerSpec) -> String {
    if spec.port_auto {
        format!("auto:{}", spec.port)
    } else {
        spec.port.to_string()
    }
}

pub(super) fn render_cloud_auth_preflight(provider: Provider, source: AuthSecretSource) {
    println!(
        "{} {} key verified from {} for cloud runtime.",
        style("verified").green().bold(),
        provider.display_name(),
        source
    );
}
