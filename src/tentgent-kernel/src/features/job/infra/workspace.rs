use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    features::job::{
        domain::{
            JobId, JobItem, JobResultFile, JobResultFileList, JobWorkspaceStreamSummary,
            JobWorkspaceSummary,
        },
        ports::{
            JobChunkCursor, JobChunkPort, JobChunkRead, JobChunkWrite, JobResultPort,
            JobStreamKind, JobWorkspacePort, JobWorkspaceRef,
        },
    },
    foundation::error::KernelResult,
};

use super::error::job_store_error;

const RESULTS_MANIFEST: &str = "results.toml";
const DEFAULT_WORKSPACE_RETENTION_SECONDS: i64 = 10 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobWorkspaceGcPolicy {
    pub terminal_retention_seconds: i64,
    pub orphan_retention_seconds: i64,
}

impl Default for JobWorkspaceGcPolicy {
    fn default() -> Self {
        Self {
            terminal_retention_seconds: DEFAULT_WORKSPACE_RETENTION_SECONDS,
            orphan_retention_seconds: DEFAULT_WORKSPACE_RETENTION_SECONDS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileJobWorkspaceStore {
    runtime_dir: PathBuf,
    gc_policy: JobWorkspaceGcPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamManifest {
    stream: JobStreamKind,
    summary: JobWorkspaceStreamSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ResultManifest {
    files: Vec<JobResultFile>,
}

impl FileJobWorkspaceStore {
    pub fn from_runtime_dir(runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            runtime_dir: runtime_dir.into(),
            gc_policy: JobWorkspaceGcPolicy::default(),
        }
    }

    pub fn with_gc_policy(mut self, gc_policy: JobWorkspaceGcPolicy) -> Self {
        self.gc_policy = gc_policy;
        self
    }

    fn workspace_dir(&self, job_id: &JobId) -> PathBuf {
        self.runtime_dir
            .join("jobs")
            .join(job_id.as_str())
            .join("workspace")
    }

    fn stream_dir(&self, job_id: &JobId, stream: JobStreamKind) -> PathBuf {
        self.workspace_dir(job_id).join(stream.as_str())
    }

    fn chunk_path(&self, job_id: &JobId, stream: JobStreamKind, index: u64) -> PathBuf {
        self.stream_dir(job_id, stream)
            .join(format!("{index:016x}.chunk"))
    }

    fn part_path(&self, job_id: &JobId, stream: JobStreamKind, index: u64) -> PathBuf {
        self.stream_dir(job_id, stream)
            .join(format!("{index:016x}.part"))
    }

    fn stream_manifest_path(&self, job_id: &JobId, stream: JobStreamKind) -> PathBuf {
        self.workspace_dir(job_id)
            .join(format!("{}.done.toml", stream.as_str()))
    }

    fn results_manifest_path(&self, job_id: &JobId) -> PathBuf {
        self.workspace_dir(job_id).join(RESULTS_MANIFEST)
    }

    fn result_file_path(&self, job_id: &JobId, file_id: &str) -> PathBuf {
        self.workspace_dir(job_id).join("files").join(file_id)
    }

    fn remove_workspace_dir(&self, workspace_dir: &Path) -> KernelResult<bool> {
        if !workspace_dir.exists() {
            return Ok(false);
        }
        fs::remove_dir_all(workspace_dir).map_err(|err| {
            job_store_error(format!(
                "remove `{}` failed: {err}",
                workspace_dir.display()
            ))
        })?;
        Ok(true)
    }

    fn read_stream_manifest(
        &self,
        job_id: &JobId,
        stream: JobStreamKind,
    ) -> KernelResult<Option<JobWorkspaceStreamSummary>> {
        let path = self.stream_manifest_path(job_id, stream);
        if !path.exists() {
            return Ok(None);
        }
        let body = fs::read_to_string(&path)
            .map_err(|err| job_store_error(format!("read `{}` failed: {err}", path.display())))?;
        let manifest = toml::from_str::<StreamManifest>(&body)
            .map_err(|err| job_store_error(format!("parse `{}` failed: {err}", path.display())))?;
        Ok(Some(manifest.summary))
    }

    fn summarize_stream(
        &self,
        job_id: &JobId,
        stream: JobStreamKind,
    ) -> KernelResult<Option<JobWorkspaceStreamSummary>> {
        if let Some(summary) = self.read_stream_manifest(job_id, stream)? {
            return Ok(Some(summary));
        }

        let dir = self.stream_dir(job_id, stream);
        if !dir.exists() {
            return Ok(None);
        }
        let mut chunks = Vec::new();
        for entry in fs::read_dir(&dir)
            .map_err(|err| job_store_error(format!("read `{}` failed: {err}", dir.display())))?
        {
            let path = entry
                .map_err(|err| job_store_error(format!("read `{}` failed: {err}", dir.display())))?
                .path();
            if is_chunk_path(&path) {
                chunks.push(path);
            }
        }
        if chunks.is_empty() {
            return Ok(None);
        }
        let total_bytes = chunks
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .sum();
        Ok(Some(JobWorkspaceStreamSummary {
            state: "open".to_string(),
            done: false,
            failed: false,
            chunk_count: chunks.len() as u64,
            total_bytes,
            sha256: None,
            media_type: None,
            original_filename: None,
        }))
    }

    fn read_results_manifest(&self, job_id: &JobId) -> KernelResult<ResultManifest> {
        let path = self.results_manifest_path(job_id);
        if !path.exists() {
            return Ok(ResultManifest::default());
        }
        let body = fs::read_to_string(&path)
            .map_err(|err| job_store_error(format!("read `{}` failed: {err}", path.display())))?;
        toml::from_str(&body)
            .map_err(|err| job_store_error(format!("parse `{}` failed: {err}", path.display())))
    }

    fn write_results_manifest(
        &self,
        job_id: &JobId,
        manifest: &ResultManifest,
    ) -> KernelResult<()> {
        let path = self.results_manifest_path(job_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                job_store_error(format!("create `{}` failed: {err}", parent.display()))
            })?;
        }
        let body = toml::to_string_pretty(manifest)
            .map_err(|err| job_store_error(format!("serialize result manifest failed: {err}")))?;
        fs::write(&path, body)
            .map_err(|err| job_store_error(format!("write `{}` failed: {err}", path.display())))
    }

    fn terminal_retention_elapsed(&self, job: &JobItem) -> bool {
        let Some(finished_at) = job.timing.finished_at.as_deref() else {
            return false;
        };
        let Ok(finished_at) = OffsetDateTime::parse(finished_at, &Rfc3339) else {
            return false;
        };
        finished_at + time::Duration::seconds(self.gc_policy.terminal_retention_seconds)
            < OffsetDateTime::now_utc()
    }

    fn orphan_retention_elapsed(&self, workspace_dir: &Path) -> bool {
        let Ok(metadata) = fs::metadata(workspace_dir) else {
            return false;
        };
        let Ok(modified) = metadata.modified() else {
            return false;
        };
        let modified = OffsetDateTime::from(modified);
        modified + time::Duration::seconds(self.gc_policy.orphan_retention_seconds)
            < OffsetDateTime::now_utc()
    }
}

impl JobWorkspacePort for FileJobWorkspaceStore {
    fn open_workspace(&self, job_id: &JobId) -> KernelResult<JobWorkspaceRef> {
        let workspace_dir = self.workspace_dir(job_id);
        fs::create_dir_all(&workspace_dir).map_err(|err| {
            job_store_error(format!(
                "create `{}` failed: {err}",
                workspace_dir.display()
            ))
        })?;
        Ok(JobWorkspaceRef {
            job_id: job_id.clone(),
            workspace_dir,
        })
    }

    fn summarize_workspace(&self, job_id: &JobId) -> KernelResult<JobWorkspaceSummary> {
        Ok(JobWorkspaceSummary {
            input: self.summarize_stream(job_id, JobStreamKind::Input)?,
            result: self.summarize_stream(job_id, JobStreamKind::Result)?,
            expires_at: None,
            cleanup_state: None,
        })
    }

    fn remove_workspace(&self, job: &JobItem) -> KernelResult<bool> {
        if !job.status.is_terminal() {
            return Err(job_store_error(format!(
                "remove active job workspace `{}` failed: job is active",
                job.job_id
            )));
        }
        let workspace_dir = self.workspace_dir(&job.job_id);
        self.remove_workspace_dir(&workspace_dir)
    }

    fn sweep_workspaces(&self, jobs: &[JobItem]) -> KernelResult<usize> {
        let jobs_by_id = jobs
            .iter()
            .map(|job| (job.job_id.clone(), job))
            .collect::<BTreeMap<_, _>>();
        let jobs_dir = self.runtime_dir.join("jobs");
        if !jobs_dir.exists() {
            return Ok(0);
        }

        let mut removed = 0;
        for entry in fs::read_dir(&jobs_dir).map_err(|err| {
            job_store_error(format!("read `{}` failed: {err}", jobs_dir.display()))
        })? {
            let path = entry
                .map_err(|err| {
                    job_store_error(format!("read `{}` failed: {err}", jobs_dir.display()))
                })?
                .path();
            if !path.is_dir() {
                continue;
            }
            let Some(job_id) = path
                .file_name()
                .and_then(|value| value.to_str())
                .map(JobId::new)
            else {
                continue;
            };
            let workspace_dir = self.workspace_dir(&job_id);
            let should_remove = match jobs_by_id.get(&job_id) {
                Some(job) if !job.status.is_terminal() => false,
                Some(job) => self.terminal_retention_elapsed(job),
                None => self.orphan_retention_elapsed(&workspace_dir),
            };
            if should_remove && self.remove_workspace_dir(&workspace_dir)? {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

impl JobChunkPort for FileJobWorkspaceStore {
    fn write_chunk(&self, job_id: &JobId, chunk: JobChunkWrite) -> KernelResult<()> {
        let dir = self.stream_dir(job_id, chunk.stream);
        fs::create_dir_all(&dir)
            .map_err(|err| job_store_error(format!("create `{}` failed: {err}", dir.display())))?;
        let path = self.part_path(job_id, chunk.stream, chunk.index);
        fs::write(&path, chunk.bytes)
            .map_err(|err| job_store_error(format!("write `{}` failed: {err}", path.display())))
    }

    fn commit_chunk(&self, job_id: &JobId, stream: JobStreamKind, index: u64) -> KernelResult<()> {
        let part = self.part_path(job_id, stream, index);
        let chunk = self.chunk_path(job_id, stream, index);
        fs::rename(&part, &chunk).map_err(|err| {
            job_store_error(format!(
                "replace `{}` with `{}` failed: {err}",
                part.display(),
                chunk.display()
            ))
        })
    }

    fn finalize_stream(
        &self,
        job_id: &JobId,
        stream: JobStreamKind,
        summary: JobWorkspaceStreamSummary,
    ) -> KernelResult<JobWorkspaceSummary> {
        let path = self.stream_manifest_path(job_id, stream);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                job_store_error(format!("create `{}` failed: {err}", parent.display()))
            })?;
        }
        let manifest = StreamManifest { stream, summary };
        let body = toml::to_string_pretty(&manifest)
            .map_err(|err| job_store_error(format!("serialize stream manifest failed: {err}")))?;
        fs::write(&path, body)
            .map_err(|err| job_store_error(format!("write `{}` failed: {err}", path.display())))?;
        self.summarize_workspace(job_id)
    }

    fn read_chunks(
        &self,
        job_id: &JobId,
        stream: JobStreamKind,
        cursor: JobChunkCursor,
        max_chunks: usize,
    ) -> KernelResult<JobChunkRead> {
        let dir = self.stream_dir(job_id, stream);
        if !dir.exists() {
            return Ok(JobChunkRead {
                bytes: Vec::new(),
                next_cursor: cursor,
                done: false,
                chunks_read: 0,
            });
        }
        let mut chunks = Vec::new();
        for entry in fs::read_dir(&dir)
            .map_err(|err| job_store_error(format!("read `{}` failed: {err}", dir.display())))?
        {
            let path = entry
                .map_err(|err| job_store_error(format!("read `{}` failed: {err}", dir.display())))?
                .path();
            if is_chunk_path(&path) {
                chunks.push(path);
            }
        }
        chunks.sort();
        let start = cursor.next_index.min(chunks.len() as u64) as usize;
        let end = chunks.len().min(start + max_chunks.max(1));
        let mut bytes = Vec::new();
        for path in &chunks[start..end] {
            let chunk = fs::read(path).map_err(|err| {
                job_store_error(format!("read `{}` failed: {err}", path.display()))
            })?;
            bytes.extend_from_slice(&chunk);
        }
        let next_index = end as u64;
        let done = self
            .read_stream_manifest(job_id, stream)?
            .map(|summary| summary.done && next_index >= summary.chunk_count)
            .unwrap_or(false);
        Ok(JobChunkRead {
            bytes,
            next_cursor: JobChunkCursor { next_index },
            done,
            chunks_read: end.saturating_sub(start),
        })
    }
}

impl JobResultPort for FileJobWorkspaceStore {
    fn declare_result_file(&self, job_id: &JobId, file: JobResultFile) -> KernelResult<()> {
        let mut manifest = self.read_results_manifest(job_id)?;
        manifest
            .files
            .retain(|existing| existing.file_id != file.file_id);
        manifest.files.push(file);
        manifest
            .files
            .sort_by(|left, right| left.file_id.cmp(&right.file_id));
        self.write_results_manifest(job_id, &manifest)
    }

    fn list_result_files(&self, job_id: &JobId) -> KernelResult<JobResultFileList> {
        Ok(JobResultFileList {
            files: self.read_results_manifest(job_id)?.files,
        })
    }

    fn read_result_file(&self, job_id: &JobId, file_id: &str) -> KernelResult<Vec<u8>> {
        let path = self.result_file_path(job_id, file_id);
        fs::read(&path)
            .map_err(|err| job_store_error(format!("read `{}` failed: {err}", path.display())))
    }
}

fn is_chunk_path(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("chunk")
}
