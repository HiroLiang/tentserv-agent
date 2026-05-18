use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

use serde_json::Value;

use crate::features::train::domain::{
    IndexedMetricEvent, LoraTrainMetricsTail, LoraTrainPlan, LoraTrainPlanInspection,
    LoraTrainPlanSummary, LoraTrainRun, LoraTrainRunInspection, TrainRefSelector,
    TrainRunLogMetadata, TrainRunLogTail, TrainRunWarning, TrainStoreLayout,
};
use crate::features::train::ports::{
    LoraTrainPlanStore, LoraTrainRunArtifactPaths, LoraTrainRunStore, TrainProcessProbe,
};
use crate::foundation::error::KernelResult;

use super::error::{path_error, train_store_error};
use super::process::StdTrainProcessProbe;

const RAW_LOG_ENCODING: &str = "utf-8-lossy";

/// Filesystem-backed LoRA train plan store.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileLoraTrainPlanStore;

impl LoraTrainPlanStore for FileLoraTrainPlanStore {
    fn list_plans(&self, layout: &TrainStoreLayout) -> KernelResult<Vec<LoraTrainPlanSummary>> {
        let mut plans = Vec::new();
        if !layout.plans_dir.exists() {
            return Ok(plans);
        }

        for entry in fs::read_dir(&layout.plans_dir)
            .map_err(|err| path_error("read train plan directory", &layout.plans_dir, err))?
        {
            let entry = entry.map_err(|err| {
                train_store_error(format!(
                    "read entry in train plan directory `{}` failed: {err}",
                    layout.plans_dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                path_error("read train plan entry type", entry.path().as_path(), err)
            })?;
            if !file_type.is_dir() {
                continue;
            }

            let plan_ref = entry.file_name().to_string_lossy().into_owned();
            let plan_path = layout.plan_toml_path(&plan_ref);
            if !plan_path.exists() {
                continue;
            }

            let plan = read_lora_train_plan(&plan_path)?;
            let run_count = self.count_runs(layout, &plan.plan_ref)?;
            plans.push(LoraTrainPlanSummary { plan, run_count });
        }

        plans.sort_by(|left, right| left.plan.short_ref.cmp(&right.plan.short_ref));
        Ok(plans)
    }

    fn inspect_plan(
        &self,
        layout: &TrainStoreLayout,
        selector: &TrainRefSelector,
    ) -> KernelResult<LoraTrainPlanInspection> {
        let plan = resolve_plan(layout, selector)?;
        let plan_dir = layout.plan_dir(&plan.plan_ref);
        let plan_path = layout.plan_toml_path(&plan.plan_ref);
        let runs_dir = layout.plan_runs_dir(&plan.plan_ref);
        let run_count = self.count_runs(layout, &plan.plan_ref)?;

        Ok(LoraTrainPlanInspection {
            plan,
            plan_dir,
            plan_path,
            runs_dir,
            run_count,
        })
    }

    fn load_plan(&self, layout: &TrainStoreLayout, plan_ref: &str) -> KernelResult<LoraTrainPlan> {
        read_lora_train_plan(&layout.plan_toml_path(plan_ref))
    }

    fn save_plan(&self, layout: &TrainStoreLayout, plan: &LoraTrainPlan) -> KernelResult<()> {
        let path = layout.plan_toml_path(&plan.plan_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| path_error("create train plan parent directory", parent, err))?;
        }
        let body = toml::to_string_pretty(plan)
            .map_err(|err| train_store_error(format!("serialize train plan failed: {err}")))?;
        fs::write(&path, body).map_err(|err| path_error("write train plan", &path, err))?;
        Ok(())
    }

    fn remove_plan(&self, layout: &TrainStoreLayout, plan_ref: &str) -> KernelResult<()> {
        let plan_dir = layout.plan_dir(plan_ref);
        if plan_dir.exists() {
            fs::remove_dir_all(&plan_dir)
                .map_err(|err| path_error("remove train plan directory", &plan_dir, err))?;
        }
        Ok(())
    }

    fn count_runs(&self, layout: &TrainStoreLayout, plan_ref: &str) -> KernelResult<usize> {
        let runs_dir = layout.plan_runs_dir(plan_ref);
        if !runs_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        for entry in fs::read_dir(&runs_dir)
            .map_err(|err| path_error("read train run directory", &runs_dir, err))?
        {
            let entry = entry.map_err(|err| {
                train_store_error(format!(
                    "read entry in train run directory `{}` failed: {err}",
                    runs_dir.display()
                ))
            })?;
            if entry
                .file_type()
                .map_err(|err| {
                    path_error("read train run entry type", entry.path().as_path(), err)
                })?
                .is_dir()
            {
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Filesystem-backed LoRA train run store.
#[derive(Debug, Clone, Copy)]
pub struct FileLoraTrainRunStore<P = StdTrainProcessProbe> {
    process_probe: P,
}

impl Default for FileLoraTrainRunStore<StdTrainProcessProbe> {
    fn default() -> Self {
        Self {
            process_probe: StdTrainProcessProbe,
        }
    }
}

impl<P> FileLoraTrainRunStore<P> {
    pub fn new(process_probe: P) -> Self {
        Self { process_probe }
    }
}

impl<P> LoraTrainRunStore for FileLoraTrainRunStore<P>
where
    P: TrainProcessProbe,
{
    fn initialize_run_artifacts(
        &self,
        layout: &TrainStoreLayout,
        plan_ref: &str,
        run_ref: &str,
    ) -> KernelResult<LoraTrainRunArtifactPaths> {
        let run_dir = layout.run_dir(plan_ref, run_ref);
        let run_path = layout.run_toml_path(plan_ref, run_ref);
        let metrics_path = layout.run_metrics_path(plan_ref, run_ref);
        let raw_log_path = layout.run_raw_log_path(plan_ref, run_ref);

        fs::create_dir_all(&run_dir)
            .map_err(|err| path_error("create train run directory", &run_dir, err))?;
        fs::write(&metrics_path, "")
            .map_err(|err| path_error("create train metrics", &metrics_path, err))?;
        fs::write(&raw_log_path, "")
            .map_err(|err| path_error("create train raw log", &raw_log_path, err))?;

        Ok(LoraTrainRunArtifactPaths {
            run_dir,
            run_path,
            metrics_path,
            raw_log_path,
        })
    }

    fn save_run(&self, layout: &TrainStoreLayout, run: &LoraTrainRun) -> KernelResult<()> {
        let path = layout.run_toml_path(&run.plan_ref, &run.run_ref);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| path_error("create train run parent directory", parent, err))?;
        }
        let body = toml::to_string_pretty(run)
            .map_err(|err| train_store_error(format!("serialize train run failed: {err}")))?;
        fs::write(&path, body).map_err(|err| path_error("write train run", &path, err))?;
        Ok(())
    }

    fn list_runs(&self, layout: &TrainStoreLayout) -> KernelResult<Vec<LoraTrainRunInspection>> {
        let mut runs = Vec::new();
        if !layout.plans_dir.exists() {
            return Ok(runs);
        }

        for plan_entry in fs::read_dir(&layout.plans_dir)
            .map_err(|err| path_error("read train plan directory", &layout.plans_dir, err))?
        {
            let plan_entry = plan_entry.map_err(|err| {
                train_store_error(format!(
                    "read entry in train plan directory `{}` failed: {err}",
                    layout.plans_dir.display()
                ))
            })?;
            if !plan_entry
                .file_type()
                .map_err(|err| {
                    path_error(
                        "read train plan entry type",
                        plan_entry.path().as_path(),
                        err,
                    )
                })?
                .is_dir()
            {
                continue;
            }
            let plan_ref = plan_entry.file_name().to_string_lossy().into_owned();
            let selector = TrainRefSelector::parse(&plan_ref)
                .map_err(|err| train_store_error(err.to_string()))?;
            runs.extend(self.list_plan_runs(layout, &selector)?);
        }

        sort_run_inspections(&mut runs);
        Ok(runs)
    }

    fn list_plan_runs(
        &self,
        layout: &TrainStoreLayout,
        plan_selector: &TrainRefSelector,
    ) -> KernelResult<Vec<LoraTrainRunInspection>> {
        let plan = resolve_plan(layout, plan_selector)?;
        let runs_dir = layout.plan_runs_dir(&plan.plan_ref);
        let mut runs = Vec::new();
        if !runs_dir.exists() {
            return Ok(runs);
        }

        for entry in fs::read_dir(&runs_dir)
            .map_err(|err| path_error("read train run directory", &runs_dir, err))?
        {
            let entry = entry.map_err(|err| {
                train_store_error(format!(
                    "read entry in train run directory `{}` failed: {err}",
                    runs_dir.display()
                ))
            })?;
            if !entry
                .file_type()
                .map_err(|err| {
                    path_error("read train run entry type", entry.path().as_path(), err)
                })?
                .is_dir()
            {
                continue;
            }
            let run_ref = entry.file_name().to_string_lossy().into_owned();
            runs.push(self.inspect_run_exact(layout, &plan, &run_ref)?);
        }

        sort_run_inspections(&mut runs);
        Ok(runs)
    }

    fn inspect_run(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
    ) -> KernelResult<LoraTrainRunInspection> {
        let run = self.resolve_run(layout, run_selector)?;
        let plan_selector = TrainRefSelector::parse(&run.plan_ref)
            .map_err(|err| train_store_error(err.to_string()))?;
        let plan = resolve_plan(layout, &plan_selector)?;
        self.inspect_run_from_parts(layout, plan, run)
    }

    fn metrics_tail(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
        tail: usize,
    ) -> KernelResult<LoraTrainMetricsTail> {
        let inspection = self.inspect_run(layout, run_selector)?;
        let mut total_events = 0_usize;
        let mut warnings = Vec::new();
        let mut events = VecDeque::with_capacity(tail);

        let file = match File::open(&inspection.metrics_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LoraTrainMetricsTail {
                    metrics_path: inspection.metrics_path,
                    tail,
                    total_events: 0,
                    truncated: false,
                    events: Vec::new(),
                    warnings: vec![TrainRunWarning {
                        code: "metrics_missing".to_string(),
                        message: "metrics.jsonl was not found".to_string(),
                        line: None,
                    }],
                });
            }
            Err(error) => {
                return Err(path_error(
                    "open train metrics",
                    &inspection.metrics_path,
                    error,
                ));
            }
        };

        for (line_number, line) in BufReader::new(file).lines().enumerate() {
            let line_number = line_number + 1;
            let line = line.map_err(|err| {
                path_error("read train metrics line", &inspection.metrics_path, err)
            })?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(event) => {
                    let index = total_events;
                    total_events += 1;
                    if events.len() == tail {
                        events.pop_front();
                    }
                    events.push_back(IndexedMetricEvent { index, event });
                }
                Err(error) => warnings.push(TrainRunWarning {
                    code: "malformed_metric".to_string(),
                    message: format!(
                        "failed to parse metrics.jsonl at line {line_number}: {error}"
                    ),
                    line: Some(line_number),
                }),
            }
        }

        Ok(LoraTrainMetricsTail {
            metrics_path: inspection.metrics_path,
            tail,
            total_events,
            truncated: total_events > tail,
            events: events.into_iter().collect(),
            warnings,
        })
    }

    fn raw_log_metadata(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
    ) -> KernelResult<TrainRunLogMetadata> {
        let inspection = self.inspect_run(layout, run_selector)?;
        raw_log_metadata(&inspection.raw_log_path)
    }

    fn raw_log_tail(
        &self,
        layout: &TrainStoreLayout,
        run_selector: &TrainRefSelector,
        tail_bytes: u64,
    ) -> KernelResult<TrainRunLogTail> {
        let inspection = self.inspect_run(layout, run_selector)?;
        let metadata = raw_log_metadata(&inspection.raw_log_path)?;
        if !metadata.exists {
            return Ok(TrainRunLogTail {
                metadata,
                tail_bytes,
                truncated: false,
                encoding: RAW_LOG_ENCODING,
                content: String::new(),
            });
        }

        let content = read_tail(&metadata.path, metadata.total_bytes, tail_bytes)?;
        let truncated = metadata.total_bytes > tail_bytes;
        Ok(TrainRunLogTail {
            metadata,
            tail_bytes,
            truncated,
            encoding: RAW_LOG_ENCODING,
            content,
        })
    }
}

impl<P> FileLoraTrainRunStore<P>
where
    P: TrainProcessProbe,
{
    fn inspect_run_exact(
        &self,
        layout: &TrainStoreLayout,
        plan: &LoraTrainPlan,
        run_ref: &str,
    ) -> KernelResult<LoraTrainRunInspection> {
        let run = read_lora_train_run(&layout.run_toml_path(&plan.plan_ref, run_ref))?;
        self.inspect_run_from_parts(layout, plan.clone(), run)
    }

    fn inspect_run_from_parts(
        &self,
        layout: &TrainStoreLayout,
        plan: LoraTrainPlan,
        run: LoraTrainRun,
    ) -> KernelResult<LoraTrainRunInspection> {
        let process_running = match run.pid {
            Some(pid) if run.status.is_live() => self.process_probe.is_process_running(pid)?,
            _ => false,
        };
        let stale = run.status.is_live() && run.pid.is_some() && !process_running;
        let run_dir = layout.run_dir(&run.plan_ref, &run.run_ref);
        let run_path = layout.run_toml_path(&run.plan_ref, &run.run_ref);
        let metrics_path = layout.run_metrics_path(&run.plan_ref, &run.run_ref);
        let raw_log_path = layout.run_raw_log_path(&run.plan_ref, &run.run_ref);

        Ok(LoraTrainRunInspection {
            plan,
            run,
            run_dir,
            run_path,
            metrics_path,
            raw_log_path,
            process_running,
            stale,
        })
    }

    fn resolve_run(
        &self,
        layout: &TrainStoreLayout,
        selector: &TrainRefSelector,
    ) -> KernelResult<LoraTrainRun> {
        let mut matches = Vec::new();
        for inspection in self.list_runs(layout)? {
            if inspection.run.run_ref == selector.as_str()
                || inspection.run.run_ref.starts_with(selector.as_str())
            {
                matches.push(inspection.run);
            }
        }

        match matches.len() {
            0 => Err(train_store_error(format!(
                "LoRA train run reference `{}` was not found",
                selector.as_str()
            ))),
            1 => Ok(matches.remove(0)),
            _ => Err(train_store_error(format!(
                "LoRA train run reference `{}` matched multiple runs",
                selector.as_str()
            ))),
        }
    }
}

pub(super) fn resolve_plan(
    layout: &TrainStoreLayout,
    selector: &TrainRefSelector,
) -> KernelResult<LoraTrainPlan> {
    if selector.is_full_ref() {
        let exact_path = layout.plan_toml_path(selector.as_str());
        if exact_path.exists() {
            return read_lora_train_plan(&exact_path);
        }
    }

    let mut matches = Vec::new();
    if !layout.plans_dir.exists() {
        return Err(plan_not_found(selector));
    }

    for entry in fs::read_dir(&layout.plans_dir)
        .map_err(|err| path_error("read train plan directory", &layout.plans_dir, err))?
    {
        let entry = entry.map_err(|err| {
            train_store_error(format!(
                "read entry in train plan directory `{}` failed: {err}",
                layout.plans_dir.display()
            ))
        })?;
        if !entry
            .file_type()
            .map_err(|err| path_error("read train plan entry type", entry.path().as_path(), err))?
            .is_dir()
        {
            continue;
        }

        let plan_ref = entry.file_name().to_string_lossy().into_owned();
        if plan_ref.starts_with(selector.as_str()) {
            matches.push(read_lora_train_plan(&layout.plan_toml_path(&plan_ref))?);
        }
    }

    match matches.len() {
        0 => Err(plan_not_found(selector)),
        1 => Ok(matches.remove(0)),
        _ => Err(train_store_error(format!(
            "LoRA train plan reference `{}` matched multiple plans",
            selector.as_str()
        ))),
    }
}

pub(super) fn read_lora_train_plan(path: &Path) -> KernelResult<LoraTrainPlan> {
    let body = fs::read_to_string(path).map_err(|err| path_error("read train plan", path, err))?;
    toml::from_str(&body).map_err(|err| {
        train_store_error(format!(
            "failed to parse training metadata at `{}`: {err}",
            path.display()
        ))
    })
}

fn read_lora_train_run(path: &Path) -> KernelResult<LoraTrainRun> {
    let body = fs::read_to_string(path).map_err(|err| path_error("read train run", path, err))?;
    toml::from_str(&body).map_err(|err| {
        train_store_error(format!(
            "failed to parse training metadata at `{}`: {err}",
            path.display()
        ))
    })
}

fn sort_run_inspections(runs: &mut [LoraTrainRunInspection]) {
    runs.sort_by(|left, right| {
        right
            .run
            .created_at
            .cmp(&left.run.created_at)
            .then_with(|| left.run.run_ref.cmp(&right.run.run_ref))
    });
}

fn raw_log_metadata(path: &Path) -> KernelResult<TrainRunLogMetadata> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TrainRunLogMetadata {
                path: path.to_path_buf(),
                exists: false,
                total_bytes: 0,
                modified_at: None,
            });
        }
        Err(error) => return Err(path_error("read raw log metadata", path, error)),
    };

    let modified_at = metadata
        .modified()
        .ok()
        .map(time::OffsetDateTime::from)
        .map(|time| time.format(&time::format_description::well_known::Rfc3339))
        .transpose()
        .map_err(|err| train_store_error(format!("format raw log timestamp failed: {err}")))?;

    Ok(TrainRunLogMetadata {
        path: path.to_path_buf(),
        exists: true,
        total_bytes: metadata.len(),
        modified_at,
    })
}

fn read_tail(path: &Path, total_bytes: u64, tail_bytes: u64) -> KernelResult<String> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(path_error("open raw log", path, error)),
    };

    file.seek(SeekFrom::Start(total_bytes.saturating_sub(tail_bytes)))
        .map_err(|err| path_error("seek raw log", path, err))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|err| path_error("read raw log", path, err))?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn plan_not_found(selector: &TrainRefSelector) -> crate::foundation::error::KernelError {
    train_store_error(format!(
        "LoRA train plan reference `{}` was not found",
        selector.as_str()
    ))
}
