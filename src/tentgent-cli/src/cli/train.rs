mod interactive;
mod render;
mod run;
mod run_render;
mod run_summary;

use miette::{miette, IntoDiagnostic, Result};
use tentgent_kernel::features::adapter::infra::{
    FileAdapterBaseIndexStore, FileAdapterCatalogStore, FileAdapterContentStore,
    FileAdapterSourceIndexStore, StdAdapterIdentityGenerator, StdAdapterManifestBuilder,
    StdAdapterSourceMetadataReader, StdAdapterSourceStager, StdAdapterStoreLayoutInitializer,
};
use tentgent_kernel::features::adapter::usecases::StdAdapterTrainRunImportUseCase;
use tentgent_kernel::features::dataset::domain::DatasetRefSelector;
use tentgent_kernel::features::dataset::infra::FileDatasetCatalogStore;
use tentgent_kernel::features::model::domain::ModelRefSelector;
use tentgent_kernel::features::model::infra::FileModelCatalogStore;
use tentgent_kernel::features::runtime::infra::{
    ModelRuntimeDaemonSupervisor, StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::{
    RuntimeResolutionRequest, RuntimeResolutionUseCase, StdRuntimeResolutionUseCase,
};
use tentgent_kernel::features::train::domain::TrainRefSelector;
use tentgent_kernel::features::train::infra::{
    FileLoraTrainPlanStore, FileLoraTrainRunStore, StdLoraTrainRunRefGenerator,
    StdTrainStoreLayoutInitializer, SystemTrainClock,
};
use tentgent_kernel::features::train::usecases::{
    LoraTrainPlanBuildRequest, LoraTrainPlanInspectRequest, LoraTrainPlanListRequest,
    LoraTrainPlanRemoveRequest, LoraTrainPlanUseCase, StdLoraTrainPlanUseCase,
    StdLoraTrainRunUseCase,
};
use tentgent_kernel::foundation::error::KernelError;
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayoutInput, StdRuntimeLayoutResolver,
};
use tentgent_kernel::foundation::platform::StdPlatformProbe;

use self::interactive::{collect_overrides, confirm_save_plan};
use self::{
    render::{
        render_plan_create_outcome, render_plan_inspection, render_plan_list, render_plan_removal,
        render_plan_review,
    },
    run::run_lora_plan,
};
use super::commands::{TrainCommands, TrainLoraCommands, TrainLoraPlanCommands};

pub fn handle_train_command(action: TrainCommands) -> Result<()> {
    let train = CliTrainKernel::new();

    match action {
        TrainCommands::Lora { action } => handle_lora_command(action, &train)?,
    }

    Ok(())
}

fn handle_lora_command(action: TrainLoraCommands, train: &CliTrainKernel) -> Result<()> {
    match action {
        TrainLoraCommands::Plan { action } => handle_lora_plan_command(action, train)?,
        TrainLoraCommands::Run(command) => run_lora_plan(command, train)?,
        TrainLoraCommands::RunWorker(command) => run::run_lora_worker(command, train)?,
    }

    Ok(())
}

fn handle_lora_plan_command(action: TrainLoraPlanCommands, train: &CliTrainKernel) -> Result<()> {
    let plans = train.plan_usecase();

    match action {
        TrainLoraPlanCommands::Create(command) => {
            let backend = command.backend.into();
            let name = command.name.clone();
            let mut overrides = command.overrides();
            let model_selector =
                parse_model_selector("plan create", "--model <MODEL_REF>", &command.model)?;
            let dataset_selector =
                parse_dataset_selector("plan create", "--dataset <DATASET_REF>", &command.dataset)?;

            if command.interactive {
                let preview = plans
                    .preview_plan(LoraTrainPlanBuildRequest {
                        layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                        model_selector: model_selector.clone(),
                        dataset_selector: dataset_selector.clone(),
                        requested_backend: backend,
                        name: name.clone(),
                        overrides: overrides.clone(),
                    })
                    .into_diagnostic()?;
                render_plan_review(&preview);
                overrides = collect_overrides(&preview.plan, overrides)?;
            }

            if command.review || command.interactive {
                let preview = plans
                    .preview_plan(LoraTrainPlanBuildRequest {
                        layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                        model_selector: model_selector.clone(),
                        dataset_selector: dataset_selector.clone(),
                        requested_backend: backend,
                        name: name.clone(),
                        overrides: overrides.clone(),
                    })
                    .into_diagnostic()?;
                render_plan_review(&preview);

                if !confirm_save_plan()? {
                    println!("plan not saved.");
                    println!();
                    return Ok(());
                }
            }

            let outcome = plans
                .create_plan(LoraTrainPlanBuildRequest {
                    layout: runtime_layout_input(LayoutResolveMode::Create),
                    model_selector,
                    dataset_selector,
                    requested_backend: backend,
                    name,
                    overrides,
                })
                .into_diagnostic()?;
            render_plan_create_outcome(&outcome);
        }
        TrainLoraPlanCommands::Ls => {
            let result = plans
                .list_plans(LoraTrainPlanListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                })
                .into_diagnostic()?;
            render_plan_list(&result.plans);
        }
        TrainLoraPlanCommands::Inspect { reference } => {
            let selector = parse_train_selector("plan inspect", "PLAN_REF", &reference)?;
            let result = match plans.inspect_plan(LoraTrainPlanInspectRequest {
                layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                selector,
            }) {
                Ok(result) => result,
                Err(err) => {
                    return Err(explain_train_lookup_error("plan inspect", "PLAN_REF", err))
                }
            };
            render_plan_inspection(&result.inspection);
        }
        TrainLoraPlanCommands::Rm { reference } => {
            let selector = parse_train_selector("plan rm", "PLAN_REF", &reference)?;
            let result = match plans.remove_plan(LoraTrainPlanRemoveRequest {
                layout: runtime_layout_input(LayoutResolveMode::ReadOnly),
                selector,
            }) {
                Ok(result) => result,
                Err(err) => return Err(explain_train_lookup_error("plan rm", "PLAN_REF", err)),
            };
            render_plan_removal(&result.outcome);
        }
    }

    Ok(())
}

pub(super) struct CliTrainKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    platform_probe: StdPlatformProbe,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    model_runtime_supervisor: ModelRuntimeDaemonSupervisor,
    train_initializer: StdTrainStoreLayoutInitializer,
    plan_store: FileLoraTrainPlanStore,
    run_store: FileLoraTrainRunStore,
    clock: SystemTrainClock,
    run_refs: StdLoraTrainRunRefGenerator,
    model_catalog: FileModelCatalogStore,
    dataset_catalog: FileDatasetCatalogStore,
    adapter_initializer: StdAdapterStoreLayoutInitializer,
    adapter_stager: StdAdapterSourceStager,
    adapter_manifest_builder: StdAdapterManifestBuilder,
    adapter_identity: StdAdapterIdentityGenerator,
    adapter_metadata_reader: StdAdapterSourceMetadataReader,
    adapter_catalog: FileAdapterCatalogStore,
    adapter_source_indexes: FileAdapterSourceIndexStore,
    adapter_base_indexes: FileAdapterBaseIndexStore,
    adapter_content: FileAdapterContentStore,
}

impl CliTrainKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            platform_probe: StdPlatformProbe,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            model_runtime_supervisor: ModelRuntimeDaemonSupervisor::new(),
            train_initializer: StdTrainStoreLayoutInitializer,
            plan_store: FileLoraTrainPlanStore,
            run_store: FileLoraTrainRunStore::default(),
            clock: SystemTrainClock,
            run_refs: StdLoraTrainRunRefGenerator,
            model_catalog: FileModelCatalogStore,
            dataset_catalog: FileDatasetCatalogStore,
            adapter_initializer: StdAdapterStoreLayoutInitializer,
            adapter_stager: StdAdapterSourceStager,
            adapter_manifest_builder: StdAdapterManifestBuilder,
            adapter_identity: StdAdapterIdentityGenerator,
            adapter_metadata_reader: StdAdapterSourceMetadataReader,
            adapter_catalog: FileAdapterCatalogStore,
            adapter_source_indexes: FileAdapterSourceIndexStore,
            adapter_base_indexes: FileAdapterBaseIndexStore,
            adapter_content: FileAdapterContentStore,
        }
    }

    fn plan_usecase(&self) -> StdLoraTrainPlanUseCase<'_> {
        StdLoraTrainPlanUseCase::new(
            &self.layout_resolver,
            &self.platform_probe,
            &self.train_initializer,
            &self.model_catalog,
            &self.dataset_catalog,
            &self.plan_store,
            &self.clock,
        )
    }

    pub(super) fn run_usecase(&self) -> StdLoraTrainRunUseCase<'_> {
        StdLoraTrainRunUseCase::new(
            &self.layout_resolver,
            &self.train_initializer,
            &self.plan_store,
            &self.run_store,
            &self.clock,
            &self.run_refs,
        )
    }

    pub(super) fn adapter_train_import_usecase(&self) -> StdAdapterTrainRunImportUseCase<'_> {
        StdAdapterTrainRunImportUseCase::new(
            &self.layout_resolver,
            &self.adapter_initializer,
            &self.adapter_stager,
            &self.adapter_manifest_builder,
            &self.adapter_identity,
            &self.adapter_metadata_reader,
            &self.adapter_catalog,
            &self.adapter_source_indexes,
            &self.adapter_base_indexes,
            &self.adapter_content,
            &self.model_catalog,
        )
    }

    pub(super) fn resolve_runtime(
        &self,
        layout: RuntimeLayoutInput,
    ) -> Result<tentgent_kernel::features::runtime::usecases::RuntimeResolutionResult> {
        StdRuntimeResolutionUseCase::new(&self.layout_resolver, &self.runtime_resolver)
            .resolve_runtime(RuntimeResolutionRequest {
                layout,
                runtime: Default::default(),
            })
            .into_diagnostic()
    }
}

pub(super) fn runtime_layout_input(mode: LayoutResolveMode) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: None,
        data_root_dir: None,
    }
}

pub(super) fn runtime_layout_input_with_home(
    mode: LayoutResolveMode,
    home_dir: std::path::PathBuf,
) -> RuntimeLayoutInput {
    RuntimeLayoutInput {
        mode,
        home_dir: Some(home_dir),
        data_root_dir: None,
    }
}

fn parse_model_selector(command: &str, value_name: &str, value: &str) -> Result<ModelRefSelector> {
    ModelRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn parse_dataset_selector(
    command: &str,
    value_name: &str,
    value: &str,
) -> Result<DatasetRefSelector> {
    DatasetRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

pub(super) fn parse_train_selector(
    command: &str,
    value_name: &str,
    value: &str,
) -> Result<TrainRefSelector> {
    TrainRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn explain_train_lookup_error(command: &str, value_name: &str, err: KernelError) -> miette::Report {
    let message = err.to_string();
    if message.contains(" was not found") || message.contains(" matched multiple") {
        return usage_error(command, value_name, message);
    }

    miette!("{message}")
}

fn usage_error(command: &str, value_name: &str, message: impl std::fmt::Display) -> miette::Report {
    let usage = match command {
        "plan create" => {
            "tentgent train lora plan create -m <MODEL_REF> -d <DATASET_REF> [OPTIONS]".to_string()
        }
        "plan inspect" => "tentgent train lora plan inspect <PLAN_REF>".to_string(),
        "plan rm" => "tentgent train lora plan rm <PLAN_REF>".to_string(),
        "run" => "tentgent train lora run <PLAN_REF> [-v] [-d]".to_string(),
        _ => format!("tentgent train lora {command} <{value_name}>"),
    };
    miette!(
        "{message}\n\nUsage: {usage}\nHint: use `tentgent train lora {command} --help` for the command template."
    )
}
