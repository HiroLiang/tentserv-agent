use serde::Serialize;

use crate::runtime::{
    JobArtifact, JobItem, JobOutput, JobOutputLine, JobProgress, JobStatus, JobStream, JobTarget,
    JobTiming, JobWorkspaceStreamSummary, JobWorkspaceSummary,
};

#[derive(Debug, Serialize)]
pub struct JobsResponse {
    pub jobs: Vec<JobItemResponse>,
}

#[derive(Debug, Serialize)]
pub struct JobResponse {
    pub job: JobItemResponse,
}

#[derive(Debug, Serialize)]
pub struct JobItemResponse {
    pub job_id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub stage: String,
    pub cancellable: bool,
    pub target: Option<JobTargetResponse>,
    pub artifact: Option<JobArtifactResponse>,
    pub refresh_targets: Vec<String>,
    pub progress: JobProgressResponse,
    pub output: JobOutputResponse,
    pub workspace: Option<JobWorkspaceResponse>,
    pub timing: JobTimingResponse,
    pub warning_summary: Option<String>,
    pub result_summary: Option<String>,
    pub error_summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobTargetResponse {
    pub section: String,
    pub reference: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobArtifactResponse {
    pub kind: String,
    pub reference: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobProgressResponse {
    pub bytes_done: Option<u64>,
    pub bytes_total: Option<u64>,
    pub files_done: Option<u64>,
    pub files_total: Option<u64>,
    pub percent: Option<f64>,
    pub speed_bytes_per_sec: Option<f64>,
    pub eta_seconds: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct JobOutputResponse {
    pub tail: Vec<JobOutputLineResponse>,
    pub raw_log_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobOutputLineResponse {
    pub stream: String,
    pub line: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobWorkspaceResponse {
    pub input: Option<JobWorkspaceStreamResponse>,
    pub result: Option<JobWorkspaceStreamResponse>,
    pub expires_at: Option<String>,
    pub cleanup_state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobWorkspaceStreamResponse {
    pub state: String,
    pub done: bool,
    pub failed: bool,
    pub chunk_count: u64,
    pub total_bytes: u64,
    pub sha256: Option<String>,
    pub media_type: Option<String>,
    pub original_filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JobTimingResponse {
    pub queued_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

pub fn job_item(job: JobItem) -> JobItemResponse {
    JobItemResponse {
        job_id: job.job_id.into_string(),
        kind: job.kind.into_string(),
        label: job.label,
        status: job_status(job.status).to_string(),
        stage: job.stage,
        cancellable: job.cancellable,
        target: job.target.map(job_target),
        artifact: job.artifact.map(job_artifact),
        refresh_targets: job.refresh_targets,
        progress: job_progress(job.progress),
        output: job_output(job.output),
        workspace: job.workspace.map(job_workspace),
        timing: job_timing(job.timing),
        warning_summary: job.warning_summary,
        result_summary: job.result_summary,
        error_summary: job.error_summary,
    }
}

fn job_target(target: JobTarget) -> JobTargetResponse {
    JobTargetResponse {
        section: target.section,
        reference: target.reference,
        path: target.path,
    }
}

fn job_artifact(artifact: JobArtifact) -> JobArtifactResponse {
    JobArtifactResponse {
        kind: artifact.kind,
        reference: artifact.reference,
        path: artifact.path,
    }
}

fn job_progress(progress: JobProgress) -> JobProgressResponse {
    JobProgressResponse {
        bytes_done: progress.bytes_done,
        bytes_total: progress.bytes_total,
        files_done: progress.files_done,
        files_total: progress.files_total,
        percent: progress.percent,
        speed_bytes_per_sec: progress.speed_bytes_per_sec,
        eta_seconds: progress.eta_seconds,
    }
}

fn job_output(output: JobOutput) -> JobOutputResponse {
    JobOutputResponse {
        tail: output.tail.into_iter().map(job_output_line).collect(),
        raw_log_path: output.raw_log_path,
    }
}

fn job_output_line(line: JobOutputLine) -> JobOutputLineResponse {
    JobOutputLineResponse {
        stream: job_stream(line.stream).to_string(),
        line: line.line,
        timestamp: line.timestamp,
    }
}

fn job_workspace(workspace: JobWorkspaceSummary) -> JobWorkspaceResponse {
    JobWorkspaceResponse {
        input: workspace.input.map(job_workspace_stream),
        result: workspace.result.map(job_workspace_stream),
        expires_at: workspace.expires_at,
        cleanup_state: workspace.cleanup_state,
    }
}

fn job_workspace_stream(stream: JobWorkspaceStreamSummary) -> JobWorkspaceStreamResponse {
    JobWorkspaceStreamResponse {
        state: stream.state,
        done: stream.done,
        failed: stream.failed,
        chunk_count: stream.chunk_count,
        total_bytes: stream.total_bytes,
        sha256: stream.sha256,
        media_type: stream.media_type,
        original_filename: stream.original_filename,
    }
}

fn job_timing(timing: JobTiming) -> JobTimingResponse {
    JobTimingResponse {
        queued_at: timing.queued_at,
        started_at: timing.started_at,
        updated_at: timing.updated_at,
        finished_at: timing.finished_at,
    }
}

fn job_status(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "queued",
        JobStatus::Running => "running",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
        JobStatus::Interrupted => "interrupted",
        JobStatus::Canceled => "canceled",
    }
}

fn job_stream(stream: JobStream) -> &'static str {
    match stream {
        JobStream::Stdout => "stdout",
        JobStream::Stderr => "stderr",
        JobStream::Event => "event",
        JobStream::System => "system",
    }
}
