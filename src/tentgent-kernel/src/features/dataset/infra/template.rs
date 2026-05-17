use std::fs;
use std::path::Path;

use crate::features::dataset::domain::{DatasetRenderedTemplate, DatasetTemplateRequest};
use crate::features::dataset::ports::DatasetTemplateRenderer;
use crate::features::dataset::templates::render_dataset_generation_template;
use crate::foundation::error::KernelResult;

use super::error::path_error;

/// Renders editable Markdown-backed dataset templates.
#[derive(Debug, Clone, Copy, Default)]
pub struct MarkdownDatasetTemplateRenderer;

impl DatasetTemplateRenderer for MarkdownDatasetTemplateRenderer {
    fn render_template(
        &self,
        request: &DatasetTemplateRequest,
    ) -> KernelResult<DatasetRenderedTemplate> {
        Ok(render_dataset_generation_template(request))
    }

    fn write_template(&self, template: &DatasetRenderedTemplate, path: &Path) -> KernelResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|err| path_error("create dataset template parent", parent, err))?;
            }
        }
        fs::write(path, &template.body)
            .map_err(|err| path_error("write dataset template", path, err))
    }
}
