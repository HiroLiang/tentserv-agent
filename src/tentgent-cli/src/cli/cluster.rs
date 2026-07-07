use comfy_table::{Cell, Table};
use miette::{IntoDiagnostic, Result};
use tentgent_kernel::features::cluster::domain::{
    ClusterInspection, ClusterRef, ClusterRouteKey, ClusterRouteTarget,
};
use tentgent_kernel::features::cluster::infra::{
    FileClusterCatalogStore, StdClusterStoreLayoutInitializer,
};
use tentgent_kernel::features::cluster::usecases::{
    ClusterApplyFileRequest, ClusterInspectRequest, ClusterListRequest, ClusterRemoveRequest,
    ClusterSpecUseCase, ClusterValidateFileRequest, StdClusterUseCase,
};
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::ClusterCommands;

pub fn handle_cluster_command(action: ClusterCommands) -> Result<()> {
    let cluster = CliClusterKernel::new();

    match action {
        ClusterCommands::Apply { path, home, force } => {
            let result = cluster
                .usecase()
                .apply_cluster_file(ClusterApplyFileRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create, home),
                    source_path: path,
                    force_unsafe_source: force,
                })
                .into_diagnostic()?;
            println!(
                "Applied cluster `{}`",
                result.inspection.definition.cluster_ref
            );
            render_cluster_inspection(&result.inspection);
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
            render_cluster_routes(&result.definition.routes);
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
            let result = cluster
                .usecase()
                .inspect_cluster(ClusterInspectRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
                    cluster_ref,
                })
                .into_diagnostic()?;
            render_cluster_inspection(&result.inspection);
        }
        ClusterCommands::Rm { cluster_ref, home } => {
            let cluster_ref = parse_cluster_ref(&cluster_ref)?;
            let result = cluster
                .usecase()
                .remove_cluster(ClusterRemoveRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home),
                    cluster_ref,
                })
                .into_diagnostic()?;
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
}

impl CliClusterKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            layout_initializer: StdClusterStoreLayoutInitializer,
            catalog: FileClusterCatalogStore,
            model_catalog: FileModelCatalogStore,
        }
    }

    fn usecase(&self) -> StdClusterUseCase<'_> {
        StdClusterUseCase::new(
            &self.layout_resolver,
            &self.layout_initializer,
            &self.catalog,
            &self.model_catalog,
        )
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

    let mut table = Table::new();
    table.set_header(vec!["Cluster", "Routes"]);
    for cluster in clusters {
        table.add_row(vec![
            Cell::new(cluster.cluster_ref.to_string()),
            Cell::new(route_keys_label(&cluster.route_keys)),
        ]);
    }
    println!("{table}");
}

fn render_cluster_inspection(inspection: &ClusterInspection) {
    let mut table = Table::new();
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
        Cell::new("definition_path"),
        Cell::new(inspection.definition_path.display().to_string()),
    ]);
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
    render_cluster_routes(&inspection.definition.routes);
}

fn render_cluster_routes(routes: &std::collections::BTreeMap<ClusterRouteKey, ClusterRouteTarget>) {
    if routes.is_empty() {
        return;
    }
    let mut table = Table::new();
    table.set_header(vec!["Route", "Kind", "Target", "Runtime Profile"]);
    for (route, target) in routes {
        match target {
            ClusterRouteTarget::LocalModel {
                model_ref,
                runtime_profile,
            } => table.add_row(vec![
                Cell::new(route.to_string()),
                Cell::new("local-model"),
                Cell::new(model_ref.to_string()),
                Cell::new(
                    runtime_profile
                        .as_ref()
                        .map(|profile| profile.label())
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ]),
            ClusterRouteTarget::Provider {
                provider,
                provider_model,
            } => table.add_row(vec![
                Cell::new(route.to_string()),
                Cell::new("provider"),
                Cell::new(format!("{provider}:{provider_model}")),
                Cell::new("-"),
            ]),
        };
    }
    println!("{table}");
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
