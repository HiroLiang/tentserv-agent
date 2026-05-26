use std::{io, path::Path};

use miette::{miette, IntoDiagnostic};
use serde_json::json;
use tentgent_kernel::features::embedding::domain::EmbeddingInput;
use tentgent_kernel::features::embedding::infra::{
    PythonEmbeddingModelRuntimeClient, StdEmbeddingModelResolver,
};
use tentgent_kernel::features::embedding::usecases::{
    EmbeddingPreparationRequest, EmbeddingUseCase, StdEmbeddingUseCase,
};
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::model::usecases::StdModelCatalogReadUseCase;
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    ModelRuntimeDaemonSupervisor, StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::StdRuntimeResolutionUseCase;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};

use super::commands::EmbedCommand;

pub async fn handle_embed_command(command: EmbedCommand) -> miette::Result<()> {
    let request = embedding_request(&command)?;
    let kernel = CliEmbeddingKernel::new();
    let runtime_resolution =
        StdRuntimeResolutionUseCase::new(&kernel.layout_resolver, &kernel.runtime_resolver);
    let model_catalog =
        StdModelCatalogReadUseCase::new(&kernel.layout_resolver, &kernel.model_catalog);
    let model_resolver = StdEmbeddingModelResolver::new(&model_catalog);
    let runtime_client = PythonEmbeddingModelRuntimeClient::new(
        &kernel.executable_resolver,
        &kernel.model_runtime_supervisor,
    );
    let embedding = StdEmbeddingUseCase::new(&runtime_resolution, &model_resolver, &runtime_client);

    let result = embedding
        .embed(request)
        .await
        .map_err(|err| miette!("embedding failed: {err}"))?;
    let body = json!({
        "model_ref": result.prepared.model.metadata.model_ref.to_string(),
        "data": result.response.data,
    });
    print_json(&body, command.pretty)?;
    Ok(())
}

struct CliEmbeddingKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_runtime_supervisor: ModelRuntimeDaemonSupervisor,
    model_catalog: FileModelCatalogStore,
}

impl CliEmbeddingKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_runtime_supervisor: ModelRuntimeDaemonSupervisor::new(),
            model_catalog: FileModelCatalogStore,
        }
    }
}

fn embedding_request(command: &EmbedCommand) -> miette::Result<EmbeddingPreparationRequest> {
    let model_selector = ModelRefSelector::parse(&command.model_ref)
        .map_err(|err| miette!("failed to parse model ref for embedding: {err}"))?;
    let input = EmbeddingInput::new(command.inputs.clone())
        .map_err(|err| miette!("invalid embedding input: {err}"))?;

    Ok(EmbeddingPreparationRequest {
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
