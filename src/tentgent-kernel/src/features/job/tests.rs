use std::path::PathBuf;

use super::{
    domain::{
        JobArtifact, JobId, JobItem, JobKind, JobOutput, JobOutputLine, JobProgress,
        JobProgressPatch, JobProgressUpdate, JobResultFile, JobStatus, JobStream, JobTarget,
        JobWorkspaceStreamSummary, JobWorkspaceSummary, MAX_JOB_OUTPUT_LINES,
    },
    infra::{FileJobStore, FileJobWorkspaceStore, JobRegistry, JobWorkspaceGcPolicy},
    ports::{
        JobChunkCursor, JobChunkPort, JobChunkWrite, JobResultPort, JobStreamKind, JobWorkspacePort,
    },
    usecases::{
        JobCatalogReadUseCase, JobCreateRequest, JobInspectRequest, JobLifecycleUseCase,
        JobListRequest, JobStartRequest, JobWorkspaceUpdateRequest, JobWorkspaceUseCase,
        StdJobCatalogReadUseCase, StdJobLifecycleUseCase, StdJobWorkspaceUseCase,
    },
};
use crate::features::job::ports::JobCreateRecord;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

#[test]
fn job_status_terminal_states_are_explicit() {
    assert!(!JobStatus::Queued.is_terminal());
    assert!(!JobStatus::Running.is_terminal());
    assert!(JobStatus::Succeeded.is_terminal());
    assert!(JobStatus::Failed.is_terminal());
    assert!(JobStatus::Interrupted.is_terminal());
    assert!(JobStatus::Canceled.is_terminal());
}

#[test]
fn progress_patch_calculates_percent_from_available_counts() {
    let mut progress = JobProgress::default();

    progress.apply_patch(JobProgressPatch {
        bytes_done: Some(25),
        bytes_total: Some(100),
        ..JobProgressPatch::default()
    });

    assert_eq!(progress.percent, Some(25.0));
}

#[test]
fn output_tail_is_bounded() {
    let mut output = JobOutput::default();
    for index in 0..(MAX_JOB_OUTPUT_LINES + 5) {
        output.append(JobOutputLine::new(
            JobStream::Stdout,
            format!("line {index}"),
        ));
    }

    assert_eq!(output.tail.len(), MAX_JOB_OUTPUT_LINES);
    assert_eq!(output.tail[0].line, "line 5");
}

#[test]
fn output_tail_skips_consecutive_duplicate_lines() {
    let mut output = JobOutput::default();

    output.append(JobOutputLine::new(JobStream::Event, "Downloading"));
    output.append(JobOutputLine::new(JobStream::Event, "Downloading"));
    output.append(JobOutputLine::new(JobStream::Event, "Download complete"));

    assert_eq!(output.tail.len(), 2);
    assert_eq!(output.tail[0].line, "Downloading");
    assert_eq!(output.tail[1].line, "Download complete");
}

#[test]
fn job_lifecycle_keeps_product_artifact_separate_from_job_state() {
    let mut job = JobItem::queued("job-1", JobKind::model_pull(), "Pull model", "t0")
        .with_target(JobTarget::new("models").with_reference("repo/model"));

    job.start("pulling snapshot", "t1");
    job.update_progress(
        JobProgressUpdate {
            stage: Some("downloading".to_string()),
            progress: JobProgressPatch {
                files_done: Some(1),
                files_total: Some(2),
                ..JobProgressPatch::default()
            },
            output: vec![JobOutputLine::new(
                JobStream::Event,
                "downloaded config.json",
            )],
            warning_summary: None,
        },
        "t2",
    );
    job.succeed(
        Some(JobArtifact::new("model").with_reference("abcdef123456")),
        "model imported",
        "t3",
    );

    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.progress.percent, Some(50.0));
    assert_eq!(
        job.artifact
            .as_ref()
            .and_then(|artifact| artifact.reference.as_deref()),
        Some("abcdef123456")
    );
    assert_eq!(
        job.target
            .as_ref()
            .and_then(|target| target.reference.as_deref()),
        Some("repo/model")
    );
}

#[test]
fn registry_creates_lists_and_updates_jobs() {
    let registry = JobRegistry::new();
    let job = registry.create(
        JobKind::model_pull(),
        "Pull model",
        Some(JobTarget::new("models").with_reference("repo/model")),
        ["models".to_string()],
    );

    registry.start(&job.job_id, "pulling");
    let updated = registry
        .update_progress(
            &job.job_id,
            JobProgressUpdate {
                stage: Some("downloading".to_string()),
                progress: JobProgressPatch {
                    bytes_done: Some(1),
                    bytes_total: Some(2),
                    ..JobProgressPatch::default()
                },
                output: vec![JobOutputLine::new(JobStream::Event, "downloaded")],
                warning_summary: None,
            },
        )
        .expect("updated job");
    registry.succeed(
        &job.job_id,
        Some(JobArtifact::new("model").with_reference("abcdef123456")),
        "done",
    );

    assert_eq!(updated.progress.percent, Some(50.0));
    let jobs = registry.list();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, JobStatus::Succeeded);
    assert_eq!(
        jobs[0]
            .artifact
            .as_ref()
            .and_then(|artifact| artifact.reference.as_deref()),
        Some("abcdef123456")
    );
}

#[test]
fn registry_interrupts_only_active_jobs() {
    let registry = JobRegistry::new();
    let running = registry.create(JobKind::model_pull(), "Pull model", None, Vec::new());
    let failed = registry.create(JobKind::adapter_pull(), "Pull adapter", None, Vec::new());

    registry.start(&running.job_id, "running");
    registry.fail(&failed.job_id, "failed first");

    let interrupted = registry.interrupt_active("daemon restarted");

    assert_eq!(interrupted.len(), 1);
    assert_eq!(
        registry.get(&running.job_id).expect("running job").status,
        JobStatus::Interrupted
    );
    assert_eq!(
        registry.get(&failed.job_id).expect("failed job").status,
        JobStatus::Failed
    );
}

#[test]
fn registry_loads_persisted_jobs_and_interrupts_active_ones() {
    let root = unique_temp_dir("load-interrupt");
    let store = FileJobStore::from_jobs_dir(root.join("jobs"));
    let mut running = JobItem::queued(
        "job-running",
        JobKind::model_pull(),
        "Pull model",
        "2026-05-01T00:00:00Z",
    );
    running.start("running", "2026-05-01T00:00:01Z");
    store.persist(&running).expect("persist running job");

    let registry = JobRegistry::from_store(store);
    let loaded = registry
        .get(&JobId::new("job-running"))
        .expect("loaded job");

    assert_eq!(loaded.status, JobStatus::Interrupted);
    assert_eq!(
        loaded.error_summary.as_deref(),
        Some("daemon restarted before this job completed")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn usecases_cover_catalog_lifecycle_and_workspace_boundaries() {
    let registry = JobRegistry::new();
    let lifecycle = StdJobLifecycleUseCase::new(&registry);
    let catalog = StdJobCatalogReadUseCase::new(&registry);
    let workspace = StdJobWorkspaceUseCase::new(&registry);

    let created = lifecycle
        .create_job(JobCreateRequest {
            record: JobCreateRecord {
                kind: JobKind::model_pull(),
                label: "Pull model".to_string(),
                target: Some(JobTarget::new("models").with_reference("repo/model")),
                refresh_targets: vec!["models".to_string()],
            },
        })
        .expect("create job")
        .job;

    lifecycle
        .start_job(JobStartRequest {
            job_id: created.job_id.clone(),
            stage: "pulling".to_string(),
        })
        .expect("start job");
    workspace
        .update_job_workspace(JobWorkspaceUpdateRequest {
            job_id: created.job_id.clone(),
            workspace: JobWorkspaceSummary {
                cleanup_state: Some("retained".to_string()),
                ..JobWorkspaceSummary::default()
            },
        })
        .expect("update workspace");

    let inspected = catalog
        .inspect_job(JobInspectRequest {
            job_id: created.job_id.clone(),
        })
        .expect("inspect job")
        .job
        .expect("job");
    let listed = catalog.list_jobs(JobListRequest).expect("list jobs").jobs;

    assert_eq!(inspected.status, JobStatus::Running);
    assert_eq!(
        inspected
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.cleanup_state.as_deref()),
        Some("retained")
    );
    assert_eq!(listed.len(), 1);
}

#[test]
fn file_workspace_writes_reads_finalizes_and_removes_chunks() {
    let root = unique_temp_dir("workspace");
    let runtime_dir = root.join("runtime");
    let store = FileJobWorkspaceStore::from_runtime_dir(&runtime_dir);
    let job_id = JobId::new("job-workspace-test");

    let workspace = store.open_workspace(&job_id).expect("open workspace");
    assert!(workspace
        .workspace_dir
        .ends_with("jobs/job-workspace-test/workspace"));

    store
        .write_chunk(
            &job_id,
            JobChunkWrite {
                stream: JobStreamKind::Input,
                index: 0,
                bytes: b"hello ".to_vec(),
            },
        )
        .expect("write first part");
    store
        .commit_chunk(&job_id, JobStreamKind::Input, 0)
        .expect("commit first chunk");
    store
        .write_chunk(
            &job_id,
            JobChunkWrite {
                stream: JobStreamKind::Input,
                index: 1,
                bytes: b"world".to_vec(),
            },
        )
        .expect("write second part");
    store
        .commit_chunk(&job_id, JobStreamKind::Input, 1)
        .expect("commit second chunk");
    let summary = store
        .finalize_stream(
            &job_id,
            JobStreamKind::Input,
            JobWorkspaceStreamSummary {
                state: "done".to_string(),
                done: true,
                failed: false,
                chunk_count: 2,
                total_bytes: 11,
                sha256: None,
                media_type: Some("text/plain".to_string()),
                original_filename: None,
            },
        )
        .expect("finalize input");
    assert_eq!(summary.input.expect("input summary").chunk_count, 2);

    let read = store
        .read_chunks(
            &job_id,
            JobStreamKind::Input,
            JobChunkCursor { next_index: 0 },
            8,
        )
        .expect("read chunks");
    assert_eq!(read.bytes, b"hello world");
    assert!(read.done);

    store
        .declare_result_file(
            &job_id,
            JobResultFile {
                file_id: "transcript.txt".to_string(),
                filename: "transcript.txt".to_string(),
                media_type: Some("text/plain".to_string()),
                format: Some("text".to_string()),
                total_bytes: 11,
            },
        )
        .expect("declare result file");
    let results = store.list_result_files(&job_id).expect("list result files");
    assert_eq!(results.files.len(), 1);
    assert_eq!(results.files[0].file_id, "transcript.txt");

    let mut terminal = JobItem::queued(
        job_id.clone(),
        JobKind::new("audio_transcription"),
        "transcribe",
        "2026-05-01T00:00:00Z",
    );
    terminal.succeed(None, "done", "2026-05-01T00:00:01Z");
    assert!(store.remove_workspace(&terminal).expect("remove workspace"));
    assert!(!workspace.workspace_dir.exists());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_workspace_sweep_respects_terminal_retention() {
    let root = unique_temp_dir("workspace-sweep");
    let runtime_dir = root.join("runtime");
    let store = FileJobWorkspaceStore::from_runtime_dir(&runtime_dir).with_gc_policy(
        JobWorkspaceGcPolicy {
            terminal_retention_seconds: 30,
            orphan_retention_seconds: 30,
        },
    );

    let retained_id = JobId::new("job-retained-workspace");
    let retained_workspace = store
        .open_workspace(&retained_id)
        .expect("open retained workspace");
    let old_id = JobId::new("job-old-workspace");
    let old_workspace = store.open_workspace(&old_id).expect("open old workspace");

    let mut retained = JobItem::queued(
        retained_id,
        JobKind::new("audio_transcription"),
        "transcribe",
        test_time_string(0),
    );
    retained.succeed(None, "done", test_time_string(0));

    let mut old = JobItem::queued(
        old_id,
        JobKind::new("audio_transcription"),
        "transcribe",
        test_time_string(-60),
    );
    old.succeed(None, "done", test_time_string(-60));

    let removed = store
        .sweep_workspaces(&[retained, old])
        .expect("sweep workspaces");

    assert_eq!(removed, 1);
    assert!(retained_workspace.workspace_dir.exists());
    assert!(!old_workspace.workspace_dir.exists());

    let _ = std::fs::remove_dir_all(root);
}

fn unique_temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tentgent-kernel-job-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

fn test_time_string(offset_seconds: i64) -> String {
    (OffsetDateTime::now_utc() + Duration::seconds(offset_seconds))
        .format(&Rfc3339)
        .expect("format time")
}
