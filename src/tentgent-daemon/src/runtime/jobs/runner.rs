use std::{future::Future, pin::Pin};

use tokio::task;

use super::types::{JobArtifact, JobId, JobProgressUpdate};
use super::JobRegistry;

#[derive(Debug, Clone, Copy, Default)]
pub struct JobRunner;

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
        tokio::spawn(async move {
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
        });
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
        tokio::spawn(async move {
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
        });
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
        JobRunner.spawn_blocking(
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
        JobRunner.spawn_blocking(
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
}
