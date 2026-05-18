use std::fs;
use std::path::Path;

use crate::features::dataset::domain::DatasetRef;
use crate::features::dataset::ports::DatasetReferenceGuard;
use crate::foundation::error::KernelResult;
use crate::foundation::layout::RuntimeLayout;

use super::error::{dataset_store_error, path_error};

/// Filesystem-backed guard for train plans and runs that reference datasets.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileDatasetReferenceGuard;

impl DatasetReferenceGuard for FileDatasetReferenceGuard {
    fn train_refs_for_dataset(
        &self,
        layout: &RuntimeLayout,
        dataset_ref: &DatasetRef,
    ) -> KernelResult<Vec<String>> {
        let plans_dir = layout.train_dir.join("lora/plans");
        let mut refs = Vec::new();
        if !plans_dir.exists() {
            return Ok(refs);
        }

        scan_train_references(&plans_dir, dataset_ref, &mut refs)?;
        refs.sort();
        refs.dedup();
        Ok(refs)
    }
}

fn scan_train_references(
    plans_dir: &Path,
    dataset_ref: &DatasetRef,
    refs: &mut Vec<String>,
) -> KernelResult<()> {
    for plan_entry in fs::read_dir(plans_dir)
        .map_err(|err| path_error("read train plans directory", plans_dir, err))?
    {
        let plan_entry = plan_entry.map_err(|err| {
            dataset_store_error(format!(
                "read entry in train plans directory `{}` failed: {err}",
                plans_dir.display()
            ))
        })?;
        if !plan_entry
            .file_type()
            .map_err(|err| path_error("read train plan entry type", &plan_entry.path(), err))?
            .is_dir()
        {
            continue;
        }

        let plan_ref = plan_entry.file_name().to_string_lossy().into_owned();
        let plan_path = plan_entry.path().join("plan.toml");
        if toml_dataset_ref_matches(&plan_path, dataset_ref)? {
            refs.push(format!("plan:{plan_ref}"));
        }

        let runs_dir = plan_entry.path().join("runs");
        if runs_dir.exists() {
            scan_run_references(&runs_dir, dataset_ref, refs)?;
        }
    }

    Ok(())
}

fn scan_run_references(
    runs_dir: &Path,
    dataset_ref: &DatasetRef,
    refs: &mut Vec<String>,
) -> KernelResult<()> {
    for run_entry in fs::read_dir(runs_dir)
        .map_err(|err| path_error("read train runs directory", runs_dir, err))?
    {
        let run_entry = run_entry.map_err(|err| {
            dataset_store_error(format!(
                "read entry in train runs directory `{}` failed: {err}",
                runs_dir.display()
            ))
        })?;
        if !run_entry
            .file_type()
            .map_err(|err| path_error("read train run entry type", &run_entry.path(), err))?
            .is_dir()
        {
            continue;
        }

        let run_ref = run_entry.file_name().to_string_lossy().into_owned();
        let run_path = run_entry.path().join("run.toml");
        if toml_dataset_ref_matches(&run_path, dataset_ref)? {
            refs.push(format!("run:{run_ref}"));
        }
    }

    Ok(())
}

fn toml_dataset_ref_matches(path: &Path, dataset_ref: &DatasetRef) -> KernelResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let body = fs::read_to_string(path)
        .map_err(|err| path_error("read train reference metadata", path, err))?;
    let value = toml::from_str::<toml::Value>(&body).map_err(|err| {
        dataset_store_error(format!(
            "parse train reference metadata `{}` failed: {err}",
            path.display()
        ))
    })?;
    Ok(value
        .get("dataset_ref")
        .and_then(toml::Value::as_str)
        .is_some_and(|value| value == dataset_ref.as_str()))
}
