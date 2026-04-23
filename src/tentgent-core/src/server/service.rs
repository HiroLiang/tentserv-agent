use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use hex::encode;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::model::{read_model_metadata, ModelError, ModelMetadata};

use super::{
    error::ServerError,
    store::{
        created_at_now, read_process_metadata, read_server_spec, write_process_metadata,
        write_server_spec, LaunchMode, ServerProcessMetadata, ServerSpec, ServerStorePaths,
        DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT,
    },
};

#[derive(Debug, Clone)]
pub struct ServerManager {
    paths: ServerStorePaths,
}

#[derive(Debug, Clone)]
pub struct ServerRunRequest {
    pub model_ref: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub lazy_load: bool,
    pub idle_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ServerPrepareOutcome {
    pub spec: ServerSpec,
    pub home_dir: PathBuf,
    pub server_dir: PathBuf,
    pub spec_path: PathBuf,
    pub process_path: PathBuf,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct ServerSummary {
    pub spec: ServerSpec,
    pub running: bool,
    pub process: Option<ServerProcessMetadata>,
}

#[derive(Debug, Clone)]
pub struct ServerInspection {
    pub spec: ServerSpec,
    pub home_dir: PathBuf,
    pub server_dir: PathBuf,
    pub spec_path: PathBuf,
    pub process_path: PathBuf,
    pub stdout_log_path: PathBuf,
    pub stderr_log_path: PathBuf,
    pub running: bool,
    pub process: Option<ServerProcessMetadata>,
}

#[derive(Debug, Clone)]
pub struct ServerStopOutcome {
    pub inspection: ServerInspection,
    pub stopped_pid: u32,
}

#[derive(Debug, Clone)]
pub struct ServerRemoveOutcome {
    pub inspection: ServerInspection,
}

#[derive(Debug, Serialize)]
struct ServerIdentity<'a> {
    model_ref: &'a str,
    host: &'a str,
    port: u16,
    lazy_load: bool,
    idle_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
struct ResolvedServer {
    spec: ServerSpec,
    server_dir: PathBuf,
    spec_path: PathBuf,
    process_path: PathBuf,
    stdout_log_path: PathBuf,
    stderr_log_path: PathBuf,
}

impl ServerManager {
    pub fn new(home_override: Option<&Path>) -> Result<Self, ServerError> {
        let paths = ServerStorePaths::resolve(home_override)?;
        paths.ensure_layout()?;
        Ok(Self { paths })
    }

    pub fn prepare_run(&self, request: ServerRunRequest) -> Result<ServerPrepareOutcome, ServerError> {
        let model_ref = self.resolve_model_ref(&request.model_ref)?;
        let host = normalize_host(request.host.as_deref())?;
        let port = request.port.unwrap_or(DEFAULT_SERVER_PORT);

        let server_ref = compute_server_ref(ServerIdentity {
            model_ref: &model_ref,
            host: &host,
            port,
            lazy_load: request.lazy_load,
            idle_seconds: request.idle_seconds,
        })?;
        let short_ref = server_ref.chars().take(12).collect::<String>();

        let resolved = self.resolved_paths(&server_ref, None);

        if resolved.spec_path.exists() {
            let spec = read_server_spec(&resolved.spec_path)?;
            return Ok(ServerPrepareOutcome {
                spec,
                home_dir: self.paths.home_dir.clone(),
                server_dir: resolved.server_dir,
                spec_path: resolved.spec_path,
                process_path: resolved.process_path,
                stdout_log_path: resolved.stdout_log_path,
                stderr_log_path: resolved.stderr_log_path,
                created: false,
            });
        }

        fs::create_dir_all(&resolved.server_dir)?;

        let spec = ServerSpec {
            server_ref: server_ref.clone(),
            short_ref,
            model_ref,
            host,
            port,
            lazy_load: request.lazy_load,
            idle_seconds: request.idle_seconds,
            created_at: created_at_now()?,
        };

        write_server_spec(&resolved.spec_path, &spec)?;

        Ok(ServerPrepareOutcome {
            spec,
            home_dir: self.paths.home_dir.clone(),
            server_dir: resolved.server_dir,
            spec_path: resolved.spec_path,
            process_path: resolved.process_path,
            stdout_log_path: resolved.stdout_log_path,
            stderr_log_path: resolved.stderr_log_path,
            created: true,
        })
    }

    pub fn list(&self) -> Result<Vec<ServerSummary>, ServerError> {
        let mut servers = Vec::new();
        for resolved in self.load_all_servers()? {
            let (process, running) = self.runtime_state_for(&resolved, true)?;
            servers.push(ServerSummary {
                spec: resolved.spec,
                running,
                process,
            });
        }

        servers.sort_by(|left, right| left.spec.server_ref.cmp(&right.spec.server_ref));
        Ok(servers)
    }

    pub fn list_running(&self) -> Result<Vec<ServerSummary>, ServerError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|server| server.running)
            .collect())
    }

    pub fn inspect(&self, reference: &str) -> Result<ServerInspection, ServerError> {
        let resolved = self.resolve_reference(reference)?;
        self.inspect_resolved(&resolved, true)
    }

    pub fn resolve_for_start(&self, reference: &str) -> Result<ServerInspection, ServerError> {
        let resolved = self.resolve_reference(reference)?;
        let inspection = self.inspect_resolved(&resolved, true)?;
        self.resolve_model_ref(&inspection.spec.model_ref)?;
        if inspection.running {
            return Err(ServerError::AlreadyRunning(inspection.spec.short_ref.clone()));
        }
        Ok(inspection)
    }

    pub fn record_process_start(
        &self,
        server_ref: &str,
        pid: u32,
        launch_mode: LaunchMode,
    ) -> Result<ServerInspection, ServerError> {
        let resolved = self.resolve_reference(server_ref)?;
        let (_, running) = self.runtime_state_for(&resolved, true)?;
        if running {
            return Err(ServerError::AlreadyRunning(resolved.spec.short_ref.clone()));
        }

        fs::create_dir_all(&resolved.server_dir)?;
        let metadata = ServerProcessMetadata {
            pid,
            launch_mode,
            started_at: created_at_now()?,
        };
        write_process_metadata(&resolved.process_path, &metadata)?;
        self.inspect_resolved(&resolved, true)
    }

    pub fn clear_process_if_matches(
        &self,
        server_ref: &str,
        expected_pid: Option<u32>,
    ) -> Result<(), ServerError> {
        let resolved = self.resolve_reference(server_ref)?;
        if !resolved.process_path.exists() {
            return Ok(());
        }

        if let Some(expected_pid) = expected_pid {
            let current = read_process_metadata(&resolved.process_path)?;
            if current.pid != expected_pid {
                return Ok(());
            }
        }

        fs::remove_file(&resolved.process_path)?;
        Ok(())
    }

    pub fn stop(&self, reference: &str) -> Result<ServerStopOutcome, ServerError> {
        let resolved = self.resolve_reference(reference)?;
        let inspection = self.inspect_resolved(&resolved, true)?;
        let process = inspection
            .process
            .clone()
            .ok_or_else(|| ServerError::NotRunning(inspection.spec.short_ref.clone()))?;
        if !inspection.running {
            return Err(ServerError::NotRunning(inspection.spec.short_ref.clone()));
        }

        terminate_process(process.pid)?;
        self.clear_process_if_matches(&inspection.spec.server_ref, Some(process.pid))?;
        let inspection = self.inspect_resolved(&resolved, true)?;

        Ok(ServerStopOutcome {
            inspection,
            stopped_pid: process.pid,
        })
    }

    pub fn remove(&self, reference: &str) -> Result<ServerRemoveOutcome, ServerError> {
        let resolved = self.resolve_reference(reference)?;
        let inspection = self.inspect_resolved(&resolved, true)?;
        if inspection.running {
            return Err(ServerError::AlreadyRunning(inspection.spec.short_ref.clone()));
        }

        fs::remove_dir_all(&inspection.server_dir)?;
        Ok(ServerRemoveOutcome { inspection })
    }

    fn inspect_resolved(
        &self,
        resolved: &ResolvedServer,
        cleanup_stale: bool,
    ) -> Result<ServerInspection, ServerError> {
        let (process, running) = self.runtime_state_for(resolved, cleanup_stale)?;
        Ok(ServerInspection {
            spec: resolved.spec.clone(),
            home_dir: self.paths.home_dir.clone(),
            server_dir: resolved.server_dir.clone(),
            spec_path: resolved.spec_path.clone(),
            process_path: resolved.process_path.clone(),
            stdout_log_path: resolved.stdout_log_path.clone(),
            stderr_log_path: resolved.stderr_log_path.clone(),
            running,
            process,
        })
    }

    fn runtime_state_for(
        &self,
        resolved: &ResolvedServer,
        cleanup_stale: bool,
    ) -> Result<(Option<ServerProcessMetadata>, bool), ServerError> {
        if !resolved.process_path.exists() {
            return Ok((None, false));
        }

        let process = read_process_metadata(&resolved.process_path)?;
        let running = is_process_running(process.pid)?;
        if running {
            return Ok((Some(process), true));
        }

        if cleanup_stale {
            let _ = fs::remove_file(&resolved.process_path);
            return Ok((None, false));
        }

        Ok((Some(process), false))
    }

    fn resolve_reference(&self, reference: &str) -> Result<ResolvedServer, ServerError> {
        let mut matches = Vec::new();
        for resolved in self.load_all_servers()? {
            if resolved.spec.server_ref.starts_with(reference)
                || resolved.spec.short_ref.starts_with(reference)
            {
                matches.push(resolved);
            }
        }

        match matches.len() {
            0 => Err(ServerError::NotFound(reference.to_string())),
            1 => Ok(matches.remove(0)),
            _ => Err(ServerError::AmbiguousRef(reference.to_string())),
        }
    }

    fn resolve_model_ref(&self, reference: &str) -> Result<String, ServerError> {
        let metadata = self.resolve_model_metadata(reference)?;
        Ok(metadata.model_ref)
    }

    fn resolve_model_metadata(&self, reference: &str) -> Result<ModelMetadata, ServerError> {
        let store_dir = resolve_models_store_dir(&self.paths.home_dir);
        if !store_dir.exists() {
            return Err(ModelError::NotFound(reference.to_string()).into());
        }

        let exact_path = store_dir.join(reference).join("model.toml");
        if exact_path.exists() {
            return Ok(read_model_metadata(&exact_path)?);
        }

        let mut matches = Vec::new();
        for entry in fs::read_dir(&store_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let model_ref = entry.file_name().to_string_lossy().into_owned();
            if model_ref.starts_with(reference) {
                matches.push(read_model_metadata(
                    &store_dir.join(&model_ref).join("model.toml"),
                )?);
            }
        }

        match matches.len() {
            0 => Err(ModelError::NotFound(reference.to_string()).into()),
            1 => Ok(matches.remove(0)),
            _ => Err(ModelError::AmbiguousRef(reference.to_string()).into()),
        }
    }

    fn load_all_servers(&self) -> Result<Vec<ResolvedServer>, ServerError> {
        let mut servers = Vec::new();
        if !self.paths.servers_dir.exists() {
            return Ok(servers);
        }

        for entry in fs::read_dir(&self.paths.servers_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let spec_path = path.join("server.toml");
            if !spec_path.exists() {
                continue;
            }
            let spec = read_server_spec(&spec_path)?;
            servers.push(ResolvedServer {
                process_path: path.join("process.toml"),
                stdout_log_path: path.join("stdout.log"),
                stderr_log_path: path.join("stderr.log"),
                server_dir: path,
                spec_path,
                spec,
            });
        }

        Ok(servers)
    }

    fn resolved_paths(&self, server_ref: &str, spec: Option<ServerSpec>) -> ResolvedServer {
        let server_dir = self.paths.server_dir(server_ref);
        ResolvedServer {
            process_path: self.paths.process_toml_path(server_ref),
            stdout_log_path: self.paths.stdout_log_path(server_ref),
            stderr_log_path: self.paths.stderr_log_path(server_ref),
            spec_path: self.paths.server_toml_path(server_ref),
            server_dir,
            spec: spec.unwrap_or(ServerSpec {
                server_ref: server_ref.to_string(),
                short_ref: server_ref.chars().take(12).collect(),
                model_ref: String::new(),
                host: DEFAULT_SERVER_HOST.to_string(),
                port: DEFAULT_SERVER_PORT,
                lazy_load: false,
                idle_seconds: None,
                created_at: String::new(),
            }),
        }
    }
}

fn normalize_host(value: Option<&str>) -> Result<String, ServerError> {
    let host = value.unwrap_or(DEFAULT_SERVER_HOST).trim();
    if host.is_empty() {
        return Err(ServerError::EmptyHost);
    }

    Ok(host.to_string())
}

fn compute_server_ref(identity: ServerIdentity<'_>) -> Result<String, ServerError> {
    let bytes = serde_json::to_vec(&identity)?;
    Ok(encode(Sha256::digest(bytes)))
}

fn resolve_models_store_dir(home_dir: &Path) -> PathBuf {
    read_env_path("TENTGENT_MODELS_DIR")
        .unwrap_or_else(|| home_dir.join("models"))
        .join("store")
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    let value = env::var(name).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn is_process_running(pid: u32) -> Result<bool, ServerError> {
    let status = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

fn terminate_process(pid: u32) -> Result<(), ServerError> {
    let status = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(ServerError::ProcessControl {
            message: format!("failed to send TERM to pid {pid}"),
        });
    }

    for _ in 0..30 {
        if !is_process_running(pid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(ServerError::ProcessControl {
        message: format!("pid {pid} did not exit after TERM"),
    })
}
