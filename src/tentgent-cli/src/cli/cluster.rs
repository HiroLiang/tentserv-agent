use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use miette::{IntoDiagnostic, Result};
use tentgent_kernel::features::auth::infra::{
    FileAuthMetadataStore, StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::StdAuthStatusUseCase;
use tentgent_kernel::features::cluster::domain::{
    ClusterInspection, ClusterReadinessReport, ClusterRef, ClusterRouteKey, ClusterRouteReadiness,
    ClusterRouteTarget,
};
use tentgent_kernel::features::cluster::infra::{
    FileClusterCatalogStore, FileClusterServerReferenceProbe, StdClusterStoreLayoutInitializer,
};
use tentgent_kernel::features::cluster::usecases::{
    ClusterApplyFileRequest, ClusterListRequest, ClusterReadinessInspectRequest,
    ClusterReadinessInspectResult, ClusterReadinessUseCase, ClusterRemoveRequest,
    ClusterSpecUseCase, ClusterValidateFileRequest, StdClusterReadinessUseCase, StdClusterUseCase,
};
use tentgent_kernel::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore,
};
use tentgent_kernel::features::runtime_ownership::{
    RuntimeOwnershipScope, StdRuntimeOwnershipUseCase,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
};

use super::commands::ClusterCommands;
use super::resource_mutation::project_resource_mutation;

pub async fn handle_cluster_command(action: ClusterCommands) -> Result<()> {
    let cluster = CliClusterKernel::new();

    match action {
        ClusterCommands::Run(command) => super::server::run_cluster_server(command).await?,
        ClusterCommands::Apply { path, home, force } => {
            let result = cluster
                .usecase()
                .apply_cluster_file_guarded(ClusterApplyFileRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create, home),
                    source_path: path,
                    force_unsafe_source: force,
                })
                .into_diagnostic()?;
            let result = project_resource_mutation(result)?;
            println!(
                "Applied cluster `{}`",
                result.inspection.definition.cluster_ref
            );
            render_cluster_inspection(&result.inspection, None);
        }
        ClusterCommands::Validate { path, home, force } => {
            let result = cluster
                .usecase()
                .validate_cluster_file(ClusterValidateFileRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
                    source_path: path,
                    force_unsafe_source: force,
                })
                .into_diagnostic()?;
            println!("Valid cluster `{}`", result.definition.cluster_ref);
            render_cluster_routes(&result.definition.routes, None);
        }
        ClusterCommands::Ls { home } => {
            let result = cluster
                .usecase()
                .list_clusters(ClusterListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
                })
                .into_diagnostic()?;
            render_cluster_list(&result.clusters);
        }
        ClusterCommands::Inspect { cluster_ref, home } => {
            let cluster_ref = parse_cluster_ref(&cluster_ref)?;
            let result = cluster.inspect_readiness(cluster_ref, home.as_deref())?;
            render_cluster_inspection(&result.inspection, Some(&result.readiness));
            let ownership = StdRuntimeOwnershipUseCase::default()
                .inspect_runtime_ownership_scope(
                    &result.layout,
                    RuntimeOwnershipScope::Cluster {
                        cluster_ref: result.inspection.definition.cluster_ref.clone(),
                    },
                )
                .into_diagnostic()?;
            super::runtime_ownership::render_runtime_ownership(&ownership);
        }
        ClusterCommands::Rm { cluster_ref, home } => {
            let cluster_ref = parse_cluster_ref(&cluster_ref)?;
            let result = cluster
                .usecase()
                .remove_cluster_guarded(ClusterRemoveRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
                    cluster_ref,
                })
                .into_diagnostic()?;
            let result = project_resource_mutation(result)?;
            println!(
                "Removed cluster `{}`",
                result.outcome.inspection.definition.cluster_ref
            );
        }
    }

    Ok(())
}

struct CliClusterKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    layout_initializer: StdClusterStoreLayoutInitializer,
    catalog: FileClusterCatalogStore,
    model_catalog: FileModelCatalogStore,
    model_proofs: FileModelCapabilityProofStore,
    auth_env_probe: StdAuthEnvSecretProbe,
    auth_keychain_store: SystemKeychainAuthSecretStore,
    server_refs: FileClusterServerReferenceProbe,
}

impl CliClusterKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdClusterStoreLayoutInitializer,
            catalog: FileClusterCatalogStore,
            model_catalog: FileModelCatalogStore,
            model_proofs: FileModelCapabilityProofStore,
            auth_env_probe: StdAuthEnvSecretProbe,
            auth_keychain_store: SystemKeychainAuthSecretStore::new(),
            server_refs: FileClusterServerReferenceProbe,
        }
    }

    fn usecase(&self) -> StdClusterUseCase<'_> {
        StdClusterUseCase::new_with_server_refs(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.catalog,
            &self.model_catalog,
            &self.server_refs,
        )
    }

    fn inspect_readiness(
        &self,
        cluster_ref: ClusterRef,
        home: Option<&std::path::Path>,
    ) -> Result<ClusterReadinessInspectResult> {
        let layout = self
            .layout_resolver
            .resolve(runtime_layout_input(
                LayoutResolveMode::ReadOnly,
                home.map(std::path::Path::to_path_buf),
            ))
            .into_diagnostic()?;
        let auth_metadata_store = FileAuthMetadataStore::from_layout(&layout);
        let auth_status = StdAuthStatusUseCase::new(
            &self.auth_env_probe,
            &self.auth_keychain_store,
            &auth_metadata_store,
        );
        StdClusterReadinessUseCase::new(
            &self.layout_resolver,
            &self.catalog,
            &self.model_catalog,
            &self.model_proofs,
            &auth_status,
        )
        .inspect_cluster_readiness(ClusterReadinessInspectRequest {
            layout: runtime_layout_input(
                LayoutResolveMode::ReadOnly,
                home.map(std::path::Path::to_path_buf),
            ),
            cluster_ref,
        })
        .into_diagnostic()
    }
}

fn runtime_layout_input(
    mode: LayoutResolveMode,
    home: Option<std::path::PathBuf>,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: home,
        data_root_dir: None,
    }
}

fn parse_cluster_ref(value: &str) -> Result<ClusterRef> {
    ClusterRef::parse(value).into_diagnostic()
}

fn render_cluster_list(clusters: &[tentgent_kernel::features::cluster::domain::ClusterSummary]) {
    if clusters.is_empty() {
        println!("No clusters found.");
        return;
    }

    let mut table = cluster_table();
    table.set_header(vec!["Cluster", "Routes"]);
    for cluster in clusters {
        table.add_row(vec![
            Cell::new(cluster.cluster_ref.to_string()),
            Cell::new(route_keys_label(&cluster.route_keys)),
        ]);
    }
    println!("{table}");
}

fn render_cluster_inspection(
    inspection: &ClusterInspection,
    readiness: Option<&ClusterReadinessReport>,
) {
    let mut table = cluster_table();
    table.set_header(vec!["Field", "Value"]);
    table.add_row(vec![
        Cell::new("cluster_ref"),
        Cell::new(inspection.definition.cluster_ref.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("schema_version"),
        Cell::new(inspection.definition.schema_version.to_string()),
    ]);
    table.add_row(vec![
        Cell::new("route_update_policy"),
        Cell::new(inspection.definition.route_update_policy.as_str()),
    ]);
    table.add_row(vec![
        Cell::new("definition_path"),
        Cell::new(inspection.definition_path.display().to_string()),
    ]);
    if let Some(readiness) = readiness {
        table.add_row(vec![
            Cell::new("readiness"),
            Cell::new(format!(
                "{} ({} ready, {} attention)",
                readiness.status, readiness.ready_route_count, readiness.attention_route_count
            )),
        ]);
    }
    table.add_row(vec![
        Cell::new("routes"),
        Cell::new(route_keys_label(
            &inspection
                .definition
                .routes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
        )),
    ]);
    println!("{table}");
    render_cluster_routes(&inspection.definition.routes, readiness);
    if let Some(readiness) = readiness {
        render_cluster_readiness_details(readiness);
    }
}

fn render_cluster_routes(
    routes: &std::collections::BTreeMap<ClusterRouteKey, ClusterRouteTarget>,
    readiness: Option<&ClusterReadinessReport>,
) {
    if routes.is_empty() {
        return;
    }
    let mut table = cluster_table();
    table.set_header(vec![
        "Route",
        "Kind",
        "Target",
        "Capability",
        "Runtime Profile",
        "Status",
        "Next Action",
    ]);
    for (route, target) in routes {
        let route_readiness = readiness.and_then(|readiness| {
            readiness
                .routes
                .iter()
                .find(|route_readiness| route_readiness.route == *route)
        });
        match target {
            ClusterRouteTarget::LocalModel {
                model_ref,
                runtime_profile,
            } => table.add_row(vec![
                Cell::new(route.to_string()),
                Cell::new("local-model"),
                Cell::new(model_ref.to_string()),
                Cell::new(route_readiness_capability(route, route_readiness)),
                Cell::new(
                    route_readiness
                        .map(runtime_profile_label)
                        .or_else(|| runtime_profile.as_ref().map(|profile| profile.label()))
                        .unwrap_or_else(|| "-".to_string()),
                ),
                Cell::new(route_readiness_status(route_readiness)),
                Cell::new(route_readiness_next_action(route_readiness)),
            ]),
            ClusterRouteTarget::Provider {
                provider,
                provider_model,
            } => table.add_row(vec![
                Cell::new(route.to_string()),
                Cell::new("provider"),
                Cell::new(format!("{provider}:{provider_model}")),
                Cell::new(route_readiness_capability(route, route_readiness)),
                Cell::new("-"),
                Cell::new(route_readiness_status(route_readiness)),
                Cell::new(route_readiness_next_action(route_readiness)),
            ]),
        };
    }
    println!("{table}");
}

fn route_readiness_capability(
    route: &ClusterRouteKey,
    readiness: Option<&ClusterRouteReadiness>,
) -> String {
    readiness
        .map(|readiness| readiness.capability.as_str().to_string())
        .unwrap_or_else(|| route.model_capability().as_str().to_string())
}

fn route_readiness_status(readiness: Option<&ClusterRouteReadiness>) -> String {
    readiness
        .map(|readiness| readiness.status.as_str().to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn route_readiness_next_action(readiness: Option<&ClusterRouteReadiness>) -> String {
    readiness
        .and_then(|readiness| readiness.next_actions.first())
        .map(|action| action.label.clone())
        .unwrap_or_else(|| "-".to_string())
}

fn runtime_profile_label(readiness: &ClusterRouteReadiness) -> String {
    let label = readiness
        .runtime_profile
        .effective
        .as_ref()
        .map(|profile| profile.label())
        .unwrap_or_else(|| "-".to_string());
    match readiness.runtime_profile.source.as_str() {
        "configured" => format!("configured: {label}"),
        "inferred" => format!("inferred: {label}"),
        "unavailable" => "unavailable".to_string(),
        _ => label,
    }
}

fn render_cluster_readiness_details(readiness: &ClusterReadinessReport) {
    let notable = readiness
        .routes
        .iter()
        .filter(|route| {
            route.status.needs_attention()
                || route
                    .flags
                    .iter()
                    .any(|flag| flag == "runtime-profile-inferred")
        })
        .collect::<Vec<_>>();
    if notable.is_empty() {
        return;
    }

    println!("Route details");
    for route in notable {
        println!(
            "- {}: {}",
            route.route,
            route.reason.as_deref().unwrap_or(&route.description)
        );
        if !route.flags.is_empty() {
            println!("  flags: {}", route.flags.join(", "));
        }
        for action in &route.next_actions {
            println!("  next: {}", action.label);
            if let Some(command) = action.command.as_deref() {
                println!("    {command}");
            }
            if let Some(description) = action.description.as_deref() {
                println!("    {description}");
            }
        }
    }
}

fn route_keys_label(routes: &[ClusterRouteKey]) -> String {
    if routes.is_empty() {
        return "-".to_string();
    }
    routes
        .iter()
        .map(|route| route.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn cluster_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS);
    table
}
