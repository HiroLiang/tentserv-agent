use std::fs;
use std::path::Path;

use crate::features::server::domain::{
    LaunchMode, ServerInspection, ServerProcessMetadata, ServerRef, ServerRefSelector,
    ServerRemoveOutcome, ServerSpec, ServerStoreLayout, ServerSummary,
};
use crate::features::server::ports::{ServerCatalogStore, ServerProcessProbe};
use crate::foundation::error::KernelResult;

use super::error::{path_error, server_store_error};
use super::process::StdServerProcessProbe;

/// Filesystem-backed server spec and process metadata catalog.
#[derive(Debug, Clone, Copy)]
pub struct FileServerCatalogStore<P = StdServerProcessProbe> {
    process_probe: P,
}

impl Default for FileServerCatalogStore<StdServerProcessProbe> {
    fn default() -> Self {
        Self {
            process_probe: StdServerProcessProbe,
        }
    }
}

impl<P> FileServerCatalogStore<P> {
    pub fn new(process_probe: P) -> Self {
        Self { process_probe }
    }
}

impl<P> ServerCatalogStore for FileServerCatalogStore<P>
where
    P: ServerProcessProbe,
{
    fn list_servers(&self, layout: &ServerStoreLayout) -> KernelResult<Vec<ServerSummary>> {
        let mut servers = Vec::new();
        for spec in self.load_all_specs(layout)? {
            let inspection = self.inspect_exact(layout, spec, true)?;
            servers.push(ServerSummary {
                spec: inspection.spec,
                running: inspection.running,
                process: inspection.process,
            });
        }

        servers.sort_by(|left, right| left.spec.server_ref.cmp(&right.spec.server_ref));
        Ok(servers)
    }

    fn list_running_servers(&self, layout: &ServerStoreLayout) -> KernelResult<Vec<ServerSummary>> {
        Ok(self
            .list_servers(layout)?
            .into_iter()
            .filter(|server| server.running)
            .collect())
    }

    fn inspect_server(
        &self,
        layout: &ServerStoreLayout,
        selector: &ServerRefSelector,
    ) -> KernelResult<ServerInspection> {
        let spec = self.resolve_spec(layout, selector)?;
        self.inspect_exact(layout, spec, true)
    }

    fn load_server_spec(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
    ) -> KernelResult<ServerSpec> {
        read_server_spec(&layout.server_spec_path(server_ref.as_str()))
    }

    fn save_server_spec(&self, layout: &ServerStoreLayout, spec: &ServerSpec) -> KernelResult<()> {
        let path = layout.server_spec_path(spec.server_ref.as_str());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| path_error("create server spec parent directory", parent, err))?;
        }
        let body = toml::to_string_pretty(spec)
            .map_err(|err| server_store_error(format!("serialize server spec failed: {err}")))?;
        fs::write(&path, body).map_err(|err| path_error("write server spec", &path, err))?;
        Ok(())
    }

    fn remove_server(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
    ) -> KernelResult<ServerRemoveOutcome> {
        let selector = ServerRefSelector::parse(server_ref.as_str())
            .map_err(|err| server_store_error(err.to_string()))?;
        let inspection = self.inspect_server(layout, &selector)?;
        if inspection.running {
            return Err(server_store_error(format!(
                "server `{}` is already running",
                inspection.spec.short_ref
            )));
        }

        fs::remove_dir_all(&inspection.server_dir)
            .map_err(|err| path_error("remove server directory", &inspection.server_dir, err))?;
        Ok(ServerRemoveOutcome { inspection })
    }

    fn record_process_start(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
        pid: u32,
        launch_mode: LaunchMode,
        started_at: String,
    ) -> KernelResult<ServerInspection> {
        let spec = self.load_server_spec(layout, server_ref)?;
        let inspection = self.inspect_exact(layout, spec.clone(), true)?;
        if inspection.running {
            return Err(server_store_error(format!(
                "server `{}` is already running",
                spec.short_ref
            )));
        }

        let metadata = ServerProcessMetadata {
            pid,
            launch_mode,
            started_at,
        };
        write_process_metadata(&inspection.process_path, &metadata)?;
        self.inspect_exact(layout, spec, true)
    }

    fn clear_process_if_matches(
        &self,
        layout: &ServerStoreLayout,
        server_ref: &ServerRef,
        expected_pid: Option<u32>,
    ) -> KernelResult<()> {
        let process_path = layout.process_metadata_path(server_ref.as_str());
        if !process_path.exists() {
            return Ok(());
        }

        if let Some(expected_pid) = expected_pid {
            let current = read_process_metadata(&process_path)?;
            if current.pid != expected_pid {
                return Ok(());
            }
        }

        fs::remove_file(&process_path)
            .map_err(|err| path_error("remove server process metadata", &process_path, err))
    }
}

impl<P> FileServerCatalogStore<P>
where
    P: ServerProcessProbe,
{
    fn resolve_spec(
        &self,
        layout: &ServerStoreLayout,
        selector: &ServerRefSelector,
    ) -> KernelResult<ServerSpec> {
        let mut matches = Vec::new();

        for spec in self.load_all_specs(layout)? {
            if spec.server_ref.as_str().starts_with(selector.as_str())
                || spec.short_ref.starts_with(selector.as_str())
            {
                matches.push(spec);
            }
        }

        match matches.len() {
            0 => Err(server_store_error(format!(
                "server reference `{}` was not found",
                selector.as_str()
            ))),
            1 => Ok(matches.remove(0)),
            _ => Err(server_store_error(format!(
                "server reference `{}` is ambiguous; multiple stored servers share that prefix",
                selector.as_str()
            ))),
        }
    }

    fn inspect_exact(
        &self,
        layout: &ServerStoreLayout,
        spec: ServerSpec,
        cleanup_stale: bool,
    ) -> KernelResult<ServerInspection> {
        let server_ref = spec.server_ref.as_str();
        let server_dir = layout.server_dir(server_ref);
        let spec_path = layout.server_spec_path(server_ref);
        let process_path = layout.process_metadata_path(server_ref);
        let stdout_log_path = layout.stdout_log_path(server_ref);
        let stderr_log_path = layout.stderr_log_path(server_ref);
        let (process, running) = self.runtime_state_for(&process_path, cleanup_stale)?;

        Ok(ServerInspection {
            spec,
            home_dir: layout.home_dir.clone(),
            server_dir,
            spec_path,
            process_path,
            stdout_log_path,
            stderr_log_path,
            running,
            process,
        })
    }

    fn runtime_state_for(
        &self,
        process_path: &Path,
        cleanup_stale: bool,
    ) -> KernelResult<(Option<ServerProcessMetadata>, bool)> {
        if !process_path.exists() {
            return Ok((None, false));
        }

        let process = read_process_metadata(process_path)?;
        let running = self.process_probe.is_process_running(process.pid)?;
        if running {
            return Ok((Some(process), true));
        }

        if cleanup_stale {
            let _ = fs::remove_file(process_path);
            return Ok((None, false));
        }

        Ok((Some(process), false))
    }

    fn load_all_specs(&self, layout: &ServerStoreLayout) -> KernelResult<Vec<ServerSpec>> {
        let mut servers = Vec::new();
        if !layout.servers_dir.exists() {
            return Ok(servers);
        }

        for entry in fs::read_dir(&layout.servers_dir)
            .map_err(|err| path_error("read server directory", &layout.servers_dir, err))?
        {
            let entry = entry.map_err(|err| {
                server_store_error(format!(
                    "read entry in server directory `{}` failed: {err}",
                    layout.servers_dir.display()
                ))
            })?;
            let file_type = entry
                .file_type()
                .map_err(|err| path_error("read server entry type", entry.path().as_path(), err))?;
            if !file_type.is_dir() {
                continue;
            }

            let spec_path = entry
                .path()
                .join(super::super::domain::SERVER_SPEC_FILENAME);
            if !spec_path.exists() {
                continue;
            }
            servers.push(read_server_spec(&spec_path)?);
        }

        Ok(servers)
    }
}

fn read_server_spec(path: &Path) -> KernelResult<ServerSpec> {
    let body = fs::read_to_string(path).map_err(|err| path_error("read server spec", path, err))?;
    toml::from_str(&body).map_err(|err| {
        server_store_error(format!(
            "parse server spec `{}` failed: {err}",
            path.display()
        ))
    })
}

fn write_process_metadata(path: &Path, metadata: &ServerProcessMetadata) -> KernelResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| path_error("create process metadata parent directory", parent, err))?;
    }
    let body = toml::to_string_pretty(metadata).map_err(|err| {
        server_store_error(format!("serialize server process metadata failed: {err}"))
    })?;
    fs::write(path, body).map_err(|err| path_error("write server process metadata", path, err))
}

fn read_process_metadata(path: &Path) -> KernelResult<ServerProcessMetadata> {
    let body = fs::read_to_string(path)
        .map_err(|err| path_error("read server process metadata", path, err))?;
    toml::from_str(&body).map_err(|err| {
        server_store_error(format!(
            "parse server process metadata `{}` failed: {err}",
            path.display()
        ))
    })
}
