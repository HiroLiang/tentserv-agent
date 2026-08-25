use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    net::TcpStream,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use clap::CommandFactory;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Cell, Table};
use console::style;
use miette::{miette, IntoDiagnostic};
use serde_json::Value;
use tentgent_kernel::features::auth::domain::{
    AuthEnvLoadPolicy, AuthSecretMaterial, AuthSecretSource, AuthValidationState, Provider,
};
use tentgent_kernel::features::auth::infra::{
    FileAuthMetadataStore, ProcessSessionAuthSecretCache, ReqwestAuthSecretValidator,
    StdAuthEnvSecretProbe, SystemKeychainAuthSecretStore,
};
use tentgent_kernel::features::auth::usecases::{
    AuthSecretResolutionRequest, AuthSecretResolverUseCase, AuthSecretValidationRequest,
    AuthSecretValidationUseCase, StdAuthSecretResolverUseCase, StdAuthSecretValidationUseCase,
};
use tentgent_kernel::features::cluster::domain::ClusterRef;
use tentgent_kernel::features::cluster::infra::FileClusterCatalogStore;
use tentgent_kernel::features::model::domain::{
    ModelCapabilityProofSource, ModelCapabilityProofStatus, ModelRefSelector,
};
use tentgent_kernel::features::model::infra::{
    FileModelCapabilityProofStore, FileModelCatalogStore, SystemModelClock,
};
use tentgent_kernel::features::model::usecases::{
    ModelCapabilityProofListRequest, ModelCapabilityProofRecordRequest,
    ModelCapabilityProofUseCase, ModelCatalogReadUseCase, ModelInspectRequest,
    StdModelCapabilityProofUseCase, StdModelCatalogReadUseCase,
};
use tentgent_kernel::features::runtime::domain::PythonRuntimeResolutionInput;
use tentgent_kernel::features::runtime::infra::{
    StdPythonRuntimeResolver, StdRuntimeExecutableResolver,
};
use tentgent_kernel::features::runtime::usecases::{
    RuntimeResolutionRequest, RuntimeResolutionUseCase, StdRuntimeResolutionUseCase,
};
use tentgent_kernel::features::runtime_ownership::StdRuntimeOwnershipUseCase;
use tentgent_kernel::features::server::domain::{
    CloudProvider, LaunchMode, ServerCapability, ServerInspection, ServerPrepareTarget,
    ServerRefSelector, ServerRuntimeKind, ServerSpec, ServerStopOutcome, ServerSummary,
};
use tentgent_kernel::features::server::infra::{
    FileServerCatalogStore, ServerRuntimeLaunchRequest, ServerRuntimeLauncher,
    StdServerIdentityGenerator, StdServerProcessController, StdServerStoreLayoutInitializer,
    SystemServerClock,
};
use tentgent_kernel::features::server::ports::ServerProcessController;
use tentgent_kernel::features::server::usecases::{
    ServerClearProcessRequest, ServerInspectRequest, ServerLifecycleUseCase, ServerListRequest,
    ServerPrepareRequest, ServerRecordProcessStartRequest, ServerRemoveRequest,
    ServerResolveForStartRequest, ServerSpecUseCase, ServerStopRequest, StdServerUseCase,
};
use tentgent_kernel::foundation::layout::{
    LayoutResolveMode, RuntimeLayout, RuntimeLayoutInput, RuntimeLayoutResolver,
    StdRuntimeLayoutResolver,
};

use super::app::Cli;
use super::commands::{
    CloudServerRuntimeCommand, ClusterRunCommand, ClusterServerRuntimeCommand,
    LocalServerRuntimeCommand, ServerCommands, ServerRunCommand,
};
use super::model_support::{
    model_support_diagnostic_lines, model_support_summaries_with_runtime_profile,
};
use super::resource_mutation::project_resource_mutation;

const BACKGROUND_HEALTH_STABLE: Duration = Duration::from_secs(2);
const BACKGROUND_START_OBSERVATION: Duration = Duration::from_secs(10);
const BACKGROUND_START_POLL: Duration = Duration::from_millis(100);
const BACKGROUND_PROBE_TIMEOUT: Duration = Duration::from_millis(250);
const BACKGROUND_STDERR_TAIL_BYTES: u64 = 4096;

pub async fn handle_server_command(action: ServerCommands) -> miette::Result<()> {
    let kernel = CliServerKernel::new();
    let server = kernel.server_usecase();

    match action {
        ServerCommands::Run(command) => run_server(command, &kernel, &server).await?,
        ServerCommands::Ls { home } => {
            let result = server
                .list_servers(ServerListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home.as_deref()),
                    running_only: false,
                })
                .into_diagnostic()?;
            render_server_list("Registered servers", &result.servers);
        }
        ServerCommands::Ps { home } => {
            let result = server
                .list_servers(ServerListRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home.as_deref()),
                    running_only: true,
                })
                .into_diagnostic()?;
            render_server_list("Running servers", &result.servers);
        }
        ServerCommands::Inspect { reference, home } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("inspect")?;
                return Ok(());
            }

            let selector = parse_server_selector("inspect", "SERVER_REF", &reference)?;
            let result = server
                .inspect_server(ServerInspectRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home.as_deref()),
                    selector,
                })
                .into_diagnostic()?;
            let model_support =
                server_model_support_lines(&kernel, &result.layout, &result.inspection);
            render_server_inspection(
                "Server inspection",
                &result.inspection,
                model_support.as_deref(),
            );
            let ownership = StdRuntimeOwnershipUseCase::default()
                .inspect_runtime_ownership_scope(
                    &result.layout,
                    server
                        .runtime_ownership_scope_for_server(&result.layout, &result.inspection.spec)
                        .into_diagnostic()?,
                )
                .into_diagnostic()?;
            super::runtime_ownership::render_runtime_ownership(&ownership);
        }
        ServerCommands::Start {
            reference,
            home,
            details,
            allow_unverified,
        } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("start")?;
                return Ok(());
            }

            let selector = parse_server_selector("start", "SERVER_REF", &reference)?;
            let result = server
                .resolve_for_start(ServerResolveForStartRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home.as_deref()),
                    selector,
                    allow_unverified,
                })
                .into_diagnostic()?;
            let auth =
                resolve_server_runtime_auth(&kernel, &result.layout, &result.inspection).await?;
            if let Some(auth) = &auth {
                render_cloud_auth_preflight(auth.provider, auth.source);
            }
            let inspection = launch_background_server(
                &kernel,
                &server,
                result.layout,
                result.inspection,
                auth,
                allow_unverified,
            )
            .await?;
            render_server_started(&inspection, details);
        }
        ServerCommands::Stop {
            reference,
            home,
            details,
        } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("stop")?;
                return Ok(());
            }

            let selector = parse_server_selector("stop", "SERVER_REF", &reference)?;
            let outcome = server
                .stop_server(ServerStopRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home.as_deref()),
                    selector,
                })
                .into_diagnostic()?;
            render_server_stop(&outcome.outcome, details);
        }
        ServerCommands::Rm {
            reference,
            home,
            details,
        } => {
            if is_help_token(&reference) {
                print_server_subcommand_help("rm")?;
                return Ok(());
            }

            let selector = parse_server_selector("rm", "SERVER_REF", &reference)?;
            let outcome = server
                .remove_server_guarded(ServerRemoveRequest {
                    layout: runtime_layout_input(LayoutResolveMode::ReadOnly, home.as_deref()),
                    selector,
                })
                .into_diagnostic()?;
            let outcome = project_resource_mutation(outcome)?;
            render_server_removed(&outcome.outcome.inspection, details);
        }
    }

    Ok(())
}

pub async fn handle_cloud_server_runtime(command: CloudServerRuntimeCommand) -> miette::Result<()> {
    let _ = (command.lazy_load, command.idle_seconds);
    let provider = match command.provider.trim().to_ascii_lowercase().as_str() {
        "openai" => Provider::OpenAI,
        "anthropic" | "claude" => Provider::Anthropic,
        "gemini" | "google" => Provider::Gemini,
        other => return Err(miette!("unsupported cloud provider `{other}`")),
    };
    tentgent_daemon::server::cloud::run_cloud_server_runtime(
        tentgent_daemon::server::cloud::CloudServerRuntimeConfig {
            server_ref: command.server_ref,
            provider,
            provider_model: command.provider_model,
            host: command.host,
            port: command.port,
            runtime_home: command.home.map(|path| path.display().to_string()),
        },
    )
    .await
}

pub async fn handle_local_server_runtime(command: LocalServerRuntimeCommand) -> miette::Result<()> {
    let _ = command.lazy_load;
    let capability = ServerCapability::parse(&command.capability)
        .map_err(|err| miette!("unsupported local server capability: {err}"))?;
    tentgent_daemon::server::local::run_local_server_runtime(
        tentgent_daemon::server::local::LocalServerRuntimeConfig {
            server_ref: command.server_ref,
            capability,
            model_ref: command.model_ref,
            runtime_profile: command.runtime_profile,
            host: command.host,
            port: command.port,
            runtime_home: command.home,
            runtime_idle_seconds: command.runtime_idle_seconds,
            model_idle_seconds: command.model_idle_seconds,
        },
    )
    .await
}

pub async fn handle_cluster_server_runtime(
    command: ClusterServerRuntimeCommand,
) -> miette::Result<()> {
    let _ = command.lazy_load;
    let cluster_ref = ClusterRef::parse(&command.cluster_ref)
        .map_err(|err| miette!("invalid cluster ref: {err}"))?;
    tentgent_daemon::server::cluster::run_cluster_server_runtime(
        tentgent_daemon::server::cluster::ClusterServerRuntimeConfig {
            server_ref: command.server_ref,
            cluster_ref,
            host: command.host,
            port: command.port,
            runtime_home: command.home,
            runtime_idle_seconds: command.runtime_idle_seconds,
            model_idle_seconds: command.model_idle_seconds,
            allow_unverified: command.allow_unverified,
        },
    )
    .await
}

async fn run_server(
    command: ServerRunCommand,
    kernel: &CliServerKernel,
    server: &StdServerUseCase<'_>,
) -> miette::Result<()> {
    if is_help_token(&command.runtime_ref) {
        print_server_subcommand_help("run")?;
        return Ok(());
    }

    let runtime_idle_seconds =
        resolve_runtime_idle_alias(command.runtime_idle_seconds, command.idle_seconds)?;

    let outcome = server
        .prepare_server(ServerPrepareRequest {
            layout: runtime_layout_input(LayoutResolveMode::Create, command.home.as_deref()),
            target: ServerPrepareTarget::RuntimeRef {
                runtime_ref: command.runtime_ref,
                capability: command.capability,
            },
            host: command.host,
            port: command.port,
            lazy_load: command.lazy_load,
            idle_seconds: runtime_idle_seconds,
            model_idle_seconds: command.model_idle_seconds,
            allow_unverified: command.allow_unverified,
        })
        .into_diagnostic()?;

    let detached = command.detach;
    render_server_spec_outcome(&outcome.outcome, detached);

    let auth =
        resolve_server_runtime_auth(kernel, &outcome.layout, &outcome.outcome.inspection).await?;
    if let Some(auth) = &auth {
        render_cloud_auth_preflight(auth.provider, auth.source);
    }
    if detached {
        let inspection = launch_background_server(
            kernel,
            server,
            outcome.layout,
            outcome.outcome.inspection,
            auth,
            command.allow_unverified,
        )
        .await?;
        render_server_inspection("Server started", &inspection, None);
    } else {
        launch_foreground_server(
            kernel,
            server,
            outcome.layout,
            outcome.outcome.inspection,
            auth,
            command.allow_unverified,
        )
        .await?;
    }

    Ok(())
}

pub(super) async fn run_cluster_server(command: ClusterRunCommand) -> miette::Result<()> {
    let cluster_ref = ClusterRef::parse(&command.cluster_ref)
        .map_err(|err| miette!("invalid cluster ref: {err}"))?;
    let kernel = CliServerKernel::new();
    let server = kernel.server_usecase();
    let runtime_idle_seconds =
        resolve_runtime_idle_alias(command.runtime_idle_seconds, command.idle_seconds)?;
    let outcome = server
        .prepare_server(ServerPrepareRequest {
            layout: runtime_layout_input(LayoutResolveMode::Create, command.home.as_deref()),
            target: ServerPrepareTarget::Cluster { cluster_ref },
            host: command.host,
            port: command.port,
            lazy_load: command.lazy_load,
            idle_seconds: runtime_idle_seconds,
            model_idle_seconds: command.model_idle_seconds,
            allow_unverified: command.allow_unverified,
        })
        .into_diagnostic()?;

    render_server_spec_outcome(&outcome.outcome, command.detach);
    if command.detach {
        let inspection = launch_background_server(
            &kernel,
            &server,
            outcome.layout,
            outcome.outcome.inspection,
            None,
            command.allow_unverified,
        )
        .await?;
        render_server_inspection("Cluster server started", &inspection, None);
    } else {
        launch_foreground_server(
            &kernel,
            &server,
            outcome.layout,
            outcome.outcome.inspection,
            None,
            command.allow_unverified,
        )
        .await?;
    }

    Ok(())
}

fn resolve_runtime_idle_alias(
    canonical: Option<u64>,
    legacy: Option<u64>,
) -> miette::Result<Option<u64>> {
    match (canonical, legacy) {
        (Some(canonical), Some(legacy)) if canonical != legacy => Err(miette!(
            "--runtime-idle-seconds ({canonical}) and deprecated --idle-seconds ({legacy}) must match"
        )),
        (Some(canonical), _) => Ok(Some(canonical)),
        (None, legacy) => Ok(legacy),
    }
}

async fn launch_foreground_server(
    kernel: &CliServerKernel,
    server: &StdServerUseCase<'_>,
    layout: RuntimeLayout,
    inspection: ServerInspection,
    auth: Option<AuthSecretMaterial>,
    allow_unverified: bool,
) -> miette::Result<()> {
    let start = server
        .resolve_for_start_guarded(ServerResolveForStartRequest {
            layout: runtime_layout_input_from_layout(&layout, LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(inspection.spec.server_ref.as_str())
                .map_err(|error| miette!("invalid server ref before launch: {error}"))?,
            allow_unverified,
        })
        .into_diagnostic()?;
    let inspection = start.result.inspection;
    let start_permit = start.permit;
    let runtime = kernel.resolve_runtime(&layout)?;
    let launcher = ServerRuntimeLauncher::new(&kernel.executable_resolver);
    let mut child = match launcher.spawn_foreground(ServerRuntimeLaunchRequest {
        layout: layout.clone(),
        runtime,
        inspection: inspection.clone(),
        auth,
        allow_unverified,
    }) {
        Ok(child) => child,
        Err(err) => {
            let message = err.to_string();
            let _ = record_local_server_capability_proof(
                kernel,
                &layout,
                &inspection,
                ModelCapabilityProofStatus::Failed,
                Some(message),
            );
            return Err(err).into_diagnostic();
        }
    };
    if let Err(error) = server.record_process_start(ServerRecordProcessStartRequest {
        layout: runtime_layout_input_from_layout(&layout, LayoutResolveMode::ReadOnly),
        server_ref: inspection.spec.server_ref.clone(),
        pid: child.pid,
        process_token: Some(child.process_token.clone()),
        bound_port: child.bound_port,
        launch_mode: LaunchMode::Foreground,
    }) {
        let _ = child.terminate();
        return Err(error).into_diagnostic();
    }
    drop(start_permit);
    let _ = record_local_server_capability_proof(
        kernel,
        &layout,
        &inspection,
        ModelCapabilityProofStatus::Verified,
        None,
    );

    let status = child.wait().into_diagnostic();
    server
        .clear_process_if_matches(ServerClearProcessRequest {
            layout: runtime_layout_input_from_layout(&layout, LayoutResolveMode::ReadOnly),
            server_ref: inspection.spec.server_ref.clone(),
            expected_pid: Some(child.pid),
        })
        .into_diagnostic()?;
    let status = status?;
    if !status.success() {
        let _ = record_local_server_capability_proof(
            kernel,
            &layout,
            &inspection,
            ModelCapabilityProofStatus::Failed,
            Some(format!("server runtime exited with status {status}")),
        );
        return Err(miette!("server runtime exited with status {status}"));
    }

    Ok(())
}

async fn launch_background_server(
    kernel: &CliServerKernel,
    server: &StdServerUseCase<'_>,
    layout: RuntimeLayout,
    inspection: ServerInspection,
    auth: Option<AuthSecretMaterial>,
    allow_unverified: bool,
) -> miette::Result<ServerInspection> {
    let start = server
        .resolve_for_start_guarded(ServerResolveForStartRequest {
            layout: runtime_layout_input_from_layout(&layout, LayoutResolveMode::ReadOnly),
            selector: ServerRefSelector::parse(inspection.spec.server_ref.as_str())
                .map_err(|error| miette!("invalid server ref before launch: {error}"))?,
            allow_unverified,
        })
        .into_diagnostic()?;
    let inspection = start.result.inspection;
    let start_permit = start.permit;
    let runtime = kernel.resolve_runtime(&layout)?;
    let launcher = ServerRuntimeLauncher::new(&kernel.executable_resolver);
    let spawned = match launcher.spawn_background(ServerRuntimeLaunchRequest {
        layout: layout.clone(),
        runtime,
        inspection: inspection.clone(),
        auth,
        allow_unverified,
    }) {
        Ok(pid) => pid,
        Err(err) => {
            let message = err.to_string();
            let _ = record_local_server_capability_proof(
                kernel,
                &layout,
                &inspection,
                ModelCapabilityProofStatus::Failed,
                Some(message),
            );
            return Err(err).into_diagnostic();
        }
    };
    let recorded = match server.record_process_start(ServerRecordProcessStartRequest {
        layout: runtime_layout_input_from_layout(&layout, LayoutResolveMode::ReadOnly),
        server_ref: inspection.spec.server_ref.clone(),
        pid: spawned.pid,
        process_token: Some(spawned.process_token.clone()),
        bound_port: spawned.bound_port,
        launch_mode: LaunchMode::Background,
    }) {
        Ok(recorded) => recorded,
        Err(error) => {
            let _ = kernel
                .server_process_controller
                .terminate_process(spawned.pid);
            return Err(error).into_diagnostic();
        }
    };
    drop(start_permit);

    match verify_background_launch(server, &layout, &recorded.inspection, spawned.pid).await {
        Ok(checked) => {
            let _ = record_local_server_capability_proof(
                kernel,
                &layout,
                &checked,
                ModelCapabilityProofStatus::Verified,
                None,
            );
            Ok(checked)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = record_local_server_capability_proof(
                kernel,
                &layout,
                &recorded.inspection,
                ModelCapabilityProofStatus::Failed,
                Some(message),
            );
            Err(err)
        }
    }
}

fn record_local_server_capability_proof(
    kernel: &CliServerKernel,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
    status: ModelCapabilityProofStatus,
    error: Option<String>,
) -> miette::Result<()> {
    let Some(model_ref) = inspection.spec.local_model_ref() else {
        return Ok(());
    };
    let selector = ModelRefSelector::parse(model_ref.as_str())
        .map_err(|err| miette!("invalid model ref in server spec: {err}"))?;
    let capability = inspection.spec.capability.ok_or_else(|| {
        miette!(
            "local server spec `{}` is missing capability metadata",
            inspection.spec.short_ref
        )
    })?;
    kernel
        .model_capability_proof_usecase()
        .record_model_capability_proof(ModelCapabilityProofRecordRequest {
            layout: runtime_layout_input_from_layout(layout, LayoutResolveMode::Create),
            selector,
            capability: capability.required_model_capability(),
            status,
            source: ModelCapabilityProofSource::ServerStart,
            server_ref: Some(inspection.spec.server_ref.to_string()),
            runtime_profile: inspection
                .spec
                .runtime_profile
                .as_ref()
                .map(|profile| profile.profile_id.clone()),
            runtime_profile_version: inspection
                .spec
                .runtime_profile
                .as_ref()
                .map(|profile| profile.profile_version),
            error,
        })
        .into_diagnostic()?;
    Ok(())
}

async fn verify_background_launch(
    server: &StdServerUseCase<'_>,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
    pid: u32,
) -> miette::Result<ServerInspection> {
    let started = Instant::now();

    loop {
        let checked = inspect_exact_server(server, layout, inspection)?;
        if !checked.running {
            clear_background_process(server, layout, inspection, Some(pid))?;
            return Err(background_exit_error(inspection, pid));
        }

        match background_health_status(&checked) {
            BackgroundHealthStatus::Matches if started.elapsed() >= BACKGROUND_HEALTH_STABLE => {
                return Ok(checked);
            }
            BackgroundHealthStatus::DifferentServer(detail) => {
                clear_background_process(server, layout, inspection, Some(pid))?;
                return Err(miette!(
                    "failed to launch background server runtime: {detail}"
                ));
            }
            BackgroundHealthStatus::Matches | BackgroundHealthStatus::Unavailable => {}
        }

        if started.elapsed() >= BACKGROUND_START_OBSERVATION {
            return Ok(checked);
        }

        thread::sleep(BACKGROUND_START_POLL);
    }
}

fn inspect_exact_server(
    server: &StdServerUseCase<'_>,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
) -> miette::Result<ServerInspection> {
    let selector = ServerRefSelector::parse(inspection.spec.server_ref.as_str())
        .map_err(|err| miette!("failed to parse server ref: {err}"))?;
    Ok(server
        .inspect_server(ServerInspectRequest {
            layout: runtime_layout_input_from_layout(layout, LayoutResolveMode::ReadOnly),
            selector,
        })
        .into_diagnostic()?
        .inspection)
}

fn clear_background_process(
    server: &StdServerUseCase<'_>,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
    expected_pid: Option<u32>,
) -> miette::Result<()> {
    server
        .clear_process_if_matches(ServerClearProcessRequest {
            layout: runtime_layout_input_from_layout(layout, LayoutResolveMode::ReadOnly),
            server_ref: inspection.spec.server_ref.clone(),
            expected_pid,
        })
        .into_diagnostic()
}

async fn resolve_server_runtime_auth(
    kernel: &CliServerKernel,
    layout: &RuntimeLayout,
    inspection: &ServerInspection,
) -> miette::Result<Option<AuthSecretMaterial>> {
    if inspection.spec.runtime_kind != ServerRuntimeKind::Cloud {
        return Ok(None);
    }

    let provider = match inspection.spec.provider {
        Some(CloudProvider::OpenAI) => Provider::OpenAI,
        Some(CloudProvider::Anthropic) => Provider::Anthropic,
        Some(CloudProvider::Gemini) => Provider::Gemini,
        None => {
            return Err(miette!(
                "cloud server `{}` is missing provider metadata",
                inspection.spec.short_ref
            ))
        }
    };
    let resolver = kernel.auth_resolver_usecase();
    let metadata = FileAuthMetadataStore::from_layout(layout);
    let validator =
        StdAuthSecretValidationUseCase::new(&resolver, &kernel.auth_validator, &metadata);
    let result = validator
        .validate_secret(AuthSecretValidationRequest::new(
            AuthSecretResolutionRequest::for_secret_validation(
                provider,
                AuthEnvLoadPolicy::CwdDotenvOverride,
            ),
        ))
        .await
        .into_diagnostic()?;

    match result.validation {
        AuthValidationState::Verified => {
            let resolution = resolver
                .resolve_secret(AuthSecretResolutionRequest::for_secret_use(
                    provider,
                    AuthEnvLoadPolicy::CwdDotenvOverride,
                ))
                .into_diagnostic()?;
            resolution.secret.ok_or_else(|| {
                miette!(
                    "{} key disappeared after validation for cloud server `{}`",
                    provider.display_name(),
                    inspection.spec.short_ref
                )
            })
            .map(Some)
        }
        AuthValidationState::Missing => Err(miette!(
            "{} key is missing for cloud server `{}`; run `tentgent auth {} set` or set `{}` before launch",
            provider.display_name(),
            inspection.spec.short_ref,
            provider.cli_name(),
            provider.env_var()
        )),
        AuthValidationState::Invalid { reason } => Err(miette!(
            "{} key is invalid for cloud server `{}`: {reason}",
            provider.display_name(),
            inspection.spec.short_ref
        )),
        AuthValidationState::Unknown { reason } => Err(miette!(
            "{} key could not be verified for cloud server `{}`: {reason}",
            provider.display_name(),
            inspection.spec.short_ref
        )),
        AuthValidationState::NotChecked => Err(miette!(
            "{} key validation was not checked for cloud server `{}`",
            provider.display_name(),
            inspection.spec.short_ref
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BackgroundHealthStatus {
    Matches,
    DifferentServer(String),
    Unavailable,
}

fn background_health_status(inspection: &ServerInspection) -> BackgroundHealthStatus {
    let port = inspection.effective_port();
    let target = socket_addr_text(&inspection.spec.host, port);
    let Ok(mut stream) = TcpStream::connect(target) else {
        return BackgroundHealthStatus::Unavailable;
    };
    let _ = stream.set_read_timeout(Some(BACKGROUND_PROBE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(BACKGROUND_PROBE_TIMEOUT));
    let host = host_for_header(&inspection.spec.host, port);
    let request = format!("GET /healthz HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return BackgroundHealthStatus::Unavailable;
    }

    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return BackgroundHealthStatus::Unavailable;
    }
    if !response.starts_with("HTTP/1.1 200") && !response.starts_with("HTTP/1.0 200") {
        return BackgroundHealthStatus::Unavailable;
    }
    let Some((_, body)) = response.split_once("\r\n\r\n") else {
        return BackgroundHealthStatus::Unavailable;
    };
    let Ok(payload) = serde_json::from_str::<Value>(body.trim()) else {
        return BackgroundHealthStatus::Unavailable;
    };
    let server_ref_matches = payload
        .get("server_ref")
        .and_then(Value::as_str)
        .is_some_and(|server_ref| server_ref == inspection.spec.server_ref.as_str());
    let runtime_home_matches = payload
        .get("runtime_home")
        .and_then(Value::as_str)
        .is_some_and(|home| home == inspection.home_dir.display().to_string());

    if server_ref_matches && runtime_home_matches {
        return BackgroundHealthStatus::Matches;
    }

    let existing_server = payload
        .get("server_ref")
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    let existing_home = payload
        .get("runtime_home")
        .and_then(Value::as_str)
        .unwrap_or("(unknown)");
    BackgroundHealthStatus::DifferentServer(format!(
        "port {} on {} is already serving Tentgent server {} from runtime home {}; requested server {} from {}",
        port,
        inspection.spec.host,
        existing_server,
        existing_home,
        inspection.spec.server_ref,
        inspection.home_dir.display()
    ))
}

fn background_exit_error(inspection: &ServerInspection, pid: u32) -> miette::Report {
    let stderr_tail = read_stderr_tail(&inspection.stderr_log_path)
        .map(|tail| tail.trim().to_string())
        .unwrap_or_default();
    if stderr_tail.is_empty() {
        miette!("server runtime process pid {pid} exited shortly after launch")
    } else {
        miette!("server runtime process pid {pid} exited shortly after launch: {stderr_tail}")
    }
}

fn read_stderr_tail(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let read_from = len.saturating_sub(BACKGROUND_STDERR_TAIL_BYTES);
    file.seek(SeekFrom::Start(read_from))?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn socket_addr_text(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn host_for_header(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

struct CliServerKernel {
    layout_resolver: StdRuntimeLayoutResolver,
    runtime_resolver: StdPythonRuntimeResolver,
    executable_resolver: StdRuntimeExecutableResolver,
    auth_env_probe: StdAuthEnvSecretProbe,
    auth_keychain_store: SystemKeychainAuthSecretStore,
    auth_metadata_store: FileAuthMetadataStore,
    auth_cache: ProcessSessionAuthSecretCache,
    auth_validator: ReqwestAuthSecretValidator,
    server_initializer: StdServerStoreLayoutInitializer,
    server_identity: StdServerIdentityGenerator,
    server_catalog: FileServerCatalogStore,
    server_process_controller: StdServerProcessController,
    server_clock: SystemServerClock,
    model_catalog: FileModelCatalogStore,
    model_proofs: FileModelCapabilityProofStore,
    model_clock: SystemModelClock,
    cluster_catalog: FileClusterCatalogStore,
}

impl CliServerKernel {
    fn new() -> Self {
        Self {
            layout_resolver: StdRuntimeLayoutResolver,
            runtime_resolver: StdPythonRuntimeResolver,
            executable_resolver: StdRuntimeExecutableResolver,
            auth_env_probe: StdAuthEnvSecretProbe,
            auth_keychain_store: SystemKeychainAuthSecretStore::new(),
            auth_metadata_store: default_auth_metadata_store(),
            auth_cache: ProcessSessionAuthSecretCache::new(),
            auth_validator: ReqwestAuthSecretValidator::new()
                .expect("auth validation HTTP client should be constructible"),
            server_initializer: StdServerStoreLayoutInitializer,
            server_identity: StdServerIdentityGenerator,
            server_catalog: FileServerCatalogStore::default(),
            server_process_controller: StdServerProcessController::default(),
            server_clock: SystemServerClock,
            model_catalog: FileModelCatalogStore,
            model_proofs: FileModelCapabilityProofStore,
            model_clock: SystemModelClock,
            cluster_catalog: FileClusterCatalogStore,
        }
    }

    fn server_usecase(&self) -> StdServerUseCase<'_> {
        StdServerUseCase::new_with_cluster_catalog(
            &self.layout_resolver,
            &self.server_initializer,
            &self.model_catalog,
            &self.model_proofs,
            &self.cluster_catalog,
            &self.server_identity,
            &self.server_catalog,
            &self.server_process_controller,
            &self.server_clock,
        )
    }

    fn resolve_runtime(
        &self,
        layout: &RuntimeLayout,
    ) -> miette::Result<tentgent_kernel::features::runtime::domain::PythonRuntimeLayout> {
        Ok(
            StdRuntimeResolutionUseCase::new(&self.layout_resolver, &self.runtime_resolver)
                .resolve_runtime(RuntimeResolutionRequest {
                    layout: runtime_layout_input_from_layout(layout, LayoutResolveMode::ReadOnly),
                    runtime: PythonRuntimeResolutionInput::default(),
                })
                .into_diagnostic()?
                .runtime,
        )
    }

    fn model_capability_proof_usecase(&self) -> StdModelCapabilityProofUseCase<'_> {
        StdModelCapabilityProofUseCase::new(
            &self.layout_resolver,
            &self.model_catalog,
            &self.model_proofs,
            &self.model_clock,
        )
    }

    fn auth_resolver_usecase(&self) -> StdAuthSecretResolverUseCase<'_> {
        StdAuthSecretResolverUseCase::new(
            &self.auth_env_probe,
            &self.auth_keychain_store,
            &self.auth_metadata_store,
            &self.auth_cache,
        )
    }
}

fn default_auth_metadata_store() -> FileAuthMetadataStore {
    let layout = StdRuntimeLayoutResolver
        .resolve(RuntimeLayoutInput {
            mode: LayoutResolveMode::Create,
            home_dir: None,
            data_root_dir: None,
        })
        .expect("runtime layout should resolve for auth metadata");
    FileAuthMetadataStore::from_layout(&layout)
}

mod render;
use render::*;

fn parse_server_selector(
    command: &str,
    value_name: &str,
    value: &str,
) -> miette::Result<ServerRefSelector> {
    ServerRefSelector::parse(value).map_err(|err| usage_error(command, value_name, err))
}

fn usage_error(command: &str, value_name: &str, message: impl std::fmt::Display) -> miette::Report {
    let usage = match command {
        "run" => "tentgent server run <RUNTIME_REF> [OPTIONS]".to_string(),
        "inspect" => "tentgent server inspect <SERVER_REF>".to_string(),
        "start" => "tentgent server start <SERVER_REF>".to_string(),
        "stop" => "tentgent server stop <SERVER_REF>".to_string(),
        "rm" => "tentgent server rm <SERVER_REF>".to_string(),
        _ => format!("tentgent server {command} <{value_name}>"),
    };
    miette!(
        "{message}\n\nUsage: {usage}\nHint: use `tentgent server {command} --help` for the command template."
    )
}

fn is_help_token(value: &str) -> bool {
    matches!(value, "help" | "--help" | "-h")
}

fn print_server_subcommand_help(name: &str) -> miette::Result<()> {
    let mut root = Cli::command();
    let server = root
        .find_subcommand_mut("server")
        .ok_or_else(|| miette!("server command metadata is unavailable"))?;
    let subcommand = server
        .find_subcommand_mut(name)
        .ok_or_else(|| miette!("server subcommand `{name}` is unavailable"))?;
    subcommand.print_long_help().into_diagnostic()?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests;
