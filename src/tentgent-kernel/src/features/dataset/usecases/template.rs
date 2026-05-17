//! Dataset template use case.

use crate::features::dataset::ports::DatasetTemplateRenderer;
use crate::foundation::error::KernelResult;

use super::port::{
    DatasetTemplateRenderRequest, DatasetTemplateRenderResult, DatasetTemplateUseCase,
};

/// Standard Markdown-backed dataset template orchestration.
pub struct StdDatasetTemplateUseCase<'a> {
    renderer: &'a dyn DatasetTemplateRenderer,
}

impl<'a> StdDatasetTemplateUseCase<'a> {
    pub fn new(renderer: &'a dyn DatasetTemplateRenderer) -> Self {
        Self { renderer }
    }
}

impl DatasetTemplateUseCase for StdDatasetTemplateUseCase<'_> {
    fn render_dataset_template(
        &self,
        request: DatasetTemplateRenderRequest,
    ) -> KernelResult<DatasetTemplateRenderResult> {
        let rendered = self.renderer.render_template(&request.template)?;
        if let Some(path) = request.output_path.as_deref() {
            self.renderer.write_template(&rendered, path)?;
        }

        Ok(DatasetTemplateRenderResult {
            rendered,
            output_path: request.output_path,
        })
    }
}
