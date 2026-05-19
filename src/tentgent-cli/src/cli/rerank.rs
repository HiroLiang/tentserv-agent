use std::{io, path::Path};

use miette::{miette, IntoDiagnostic};
use serde_json::json;
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::model::usecases::StdModelCatalogReadUseCase;
use tentgent_kernel::features::rerank::domain::RerankInput;
use tentgent_kernel::features::rerank::infra::{
    PythonRerankOnceRuntimeClient, StdRerankModelResolver,
};
use tentgent_kernel::features::rerank::usecases::{
    RerankPreparationRequest, RerankUseCase, StdRerankUseCase,
};
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::RerankCommand;

pub async fn handle_rerank_command(command: RerankCommand) -> miette::Result<()> {
    let request = rerank_request(&command)?;
    let kernel = CliRerankKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdRerankModelResolver::new(&model_catalog);
    let runtime_client = PythonRerankOnceRuntimeClient::new(&kernel.executable_resolver);
    let rerank = StdRerankUseCase::new(&runtime_resolution, &model_resolver, &runtime_client);

    let result = rerank
        .rerank(request)
        .await
        .map_err(|err| miette!("rerank failed: {err}"))?;
    let body = json!({
        "model_ref": result.prepared.model.metadata.model_ref.to_string(),
        "data": result.response.data,
    });
    print_json(&body, command.pretty)?;
    Ok(())
}

struct CliRerankKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_catalog: FileModelCatalogStore,
}

impl CliRerankKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_catalog: FileModelCatalogStore,
        }
    }
}

fn rerank_request(command: &RerankCommand) -> miette::Result<RerankPreparationRequest> {
    let model_selector = ModelRefSelector::parse(&command.model_ref)
        .map_err(|err| miette!("failed to parse model ref for rerank: {err}"))?;
    let input = RerankInput::new(
        command.query.clone(),
        command.documents.clone(),
        command.top_n,
    )
    .map_err(|err| miette!("invalid rerank input: {err}"))?;

    Ok(RerankPreparationRequest {
        layout: runtime_layout_input(command.home.as_deref()),
        runtime: PythonRuntimeResolutionInput::default(),
        model_selector,
        input,
    })
}

fn runtime_layout_input(home: Option<&Path>) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode: LayoutResolveMode::ReadOnly,
        home_dir: home.map(Path::to_path_buf),
        data_root_dir: None,
    }
}

fn print_json(value: &serde_json::Value, pretty: bool) -> miette::Result<()> {
    let mut stdout = io::stdout().lock();
    if pretty {
        serde_json::to_writer_pretty(&mut stdout, value).into_diagnostic()?;
    } else {
        serde_json::to_writer(&mut stdout, value).into_diagnostic()?;
    }
    use std::io::Write as _;
    writeln!(stdout).into_diagnostic()?;
    Ok(())
}
