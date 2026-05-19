use std::{future::Future, pin::Pin};

use tokio::task;

use super::types::{JobArtifact, JobId, JobProgressUpdate};
use super::JobRegistry;
use super::{InFlightJobKind, InFlightJobRegistry};

#[derive(Debug, Clone, Default)]
pub struct JobRunner {
    in_flight: InFlightJobRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCompletion {
    pub artifact: Option<JobArtifact>,
    pub result_summary: String,
    pub warning_summary: Option<String>,
}

impl JobCompletion {
    pub fn new(result_summary: impl Into<String>) -> Self {
        Self {
            artifact: None,
            result_summary: result_summary.into(),
            warning_summary: None,
        }
    }

    pub fn with_artifact(mut self, artifact: JobArtifact) -> Self {
        self.artifact = Some(artifact);
        self
    }

    pub fn with_warning_summary(mut self, warning_summary: impl Into<String>) -> Self {
        self.warning_summary = Some(warning_summary.into());
        self
    }
}

impl JobRunner {
    pub fn new(in_flight: InFlightJobRegistry) -> Self {
        Self { in_flight }
    }

    pub fn in_flight(&self) -> &InFlightJobRegistry {
        &self.in_flight
    }

    pub fn abort(&self, job_id: &JobId) -> bool {
        self.in_flight.abort(job_id)
    }

    pub fn abort_all(&self) -> Vec<super::InFlightJob> {
        self.in_flight.abort_all()
    }

    pub fn spawn_blocking<F>(
        &self,
        registry: JobRegistry,
        job_id: JobId,
        start_stage: impl Into<String>,
        task: F,
    ) where
        F: FnOnce(JobRegistry, JobId) -> Result<JobCompletion, String> + Send + 'static,
    {
        let start_stage = start_stage.into();
        let in_flight = self.in_flight.clone();
        let tracked_job_id = job_id.clone();
        let handle = tokio::spawn(async move {
            registry.start(&job_id, start_stage);
            let blocking_registry = registry.clone();
            let blocking_job_id = job_id.clone();
            let result =
                task::spawn_blocking(move || task(blocking_registry, blocking_job_id)).await;

            match result {
                Ok(Ok(completion)) => {
                    if let Some(warning_summary) = completion.warning_summary {
                        registry.update_progress(
                            &job_id,
                            JobProgressUpdate {
                                warning_summary: Some(warning_summary),
                                ..JobProgressUpdate::default()
                            },
                        );
                    }
                    registry.succeed(&job_id, completion.artifact, completion.result_summary);
                }
                Ok(Err(error)) => {
                    registry.fail(&job_id, error);
                }
                Err(error) => {
                    registry.fail(&job_id, format!("job task failed: {error}"));
                }
            }
            in_flight.remove(&job_id);
        });
        self.in_flight.register(
            tracked_job_id,
            InFlightJobKind::BlockingTask,
            handle.abort_handle(),
        );
    }

    pub fn spawn_async<F>(
        &self,
        registry: JobRegistry,
        job_id: JobId,
        start_stage: impl Into<String>,
        task: F,
    ) where
        F: FnOnce(
                JobRegistry,
                JobId,
            )
                -> Pin<Box<dyn Future<Output = Result<JobCompletion, String>> + Send>>
            + Send
            + 'static,
    {
        let start_stage = start_stage.into();
        let in_flight = self.in_flight.clone();
        let tracked_job_id = job_id.clone();
        let handle = tokio::spawn(async move {
            registry.start(&job_id, start_stage);
            let result = task(registry.clone(), job_id.clone()).await;

            match result {
                Ok(completion) => {
                    registry.succeed(&job_id, completion.artifact, completion.result_summary);
                }
                Err(error) => {
                    registry.fail(&job_id, error);
                }
            }
            in_flight.remove(&job_id);
        });
        self.in_flight.register(
            tracked_job_id,
            InFlightJobKind::AsyncTask,
            handle.abort_handle(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::JobKind;

    #[tokio::test]
    async fn runner_marks_blocking_task_success() {
        let registry = JobRegistry::new();
        let job = registry.create(JobKind::model_pull(), "Pull model", None, Vec::new());
        let runner = JobRunner::default();
        runner.spawn_blocking(
            registry.clone(),
            job.job_id.clone(),
            "starting",
            move |_, _| Ok(JobCompletion::new("done")),
        );

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let job = registry.get(&job.job_id).expect("job");
        assert_eq!(job.status, crate::runtime::JobStatus::Succeeded);
        assert_eq!(job.result_summary.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn runner_marks_blocking_task_failure() {
        let registry = JobRegistry::new();
        let job = registry.create(JobKind::model_pull(), "Pull model", None, Vec::new());
        let runner = JobRunner::default();
        runner.spawn_blocking(
            registry.clone(),
            job.job_id.clone(),
            "starting",
            move |_, _| Err("failed".to_string()),
        );

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let job = registry.get(&job.job_id).expect("job");
        assert_eq!(job.status, crate::runtime::JobStatus::Failed);
        assert_eq!(job.error_summary.as_deref(), Some("failed"));
    }

    #[tokio::test]
    async fn runner_tracks_in_flight_jobs() {
        let registry = JobRegistry::new();
        let job = registry.create(JobKind::model_pull(), "Pull model", None, Vec::new());
        let runner = JobRunner::default();
        runner.spawn_async(registry, job.job_id.clone(), "starting", move |_, _| {
            Box::pin(async {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                Ok(JobCompletion::new("done"))
            })
        });

        assert!(runner.in_flight().is_active(&job.job_id));
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        assert!(!runner.in_flight().is_active(&job.job_id));
    }
}
