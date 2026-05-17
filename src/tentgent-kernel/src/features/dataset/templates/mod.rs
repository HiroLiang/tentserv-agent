//! Editable Markdown-backed dataset templates.

use super::domain::{DatasetRenderedTemplate, DatasetTemplateRequest, CANONICAL_CHAT_SCHEMA};

pub const DATASET_TEMPLATE_VERSION: &str = "tentgent.dataset.synth.v1";

const GENERATION_TEMPLATE: &str = include_str!("generation.md");

pub fn render_dataset_generation_template(
    request: &DatasetTemplateRequest,
) -> DatasetRenderedTemplate {
    DatasetRenderedTemplate {
        template_version: DATASET_TEMPLATE_VERSION.to_string(),
        body: render_template(
            GENERATION_TEMPLATE,
            &[
                ("{{template_version}}", DATASET_TEMPLATE_VERSION),
                ("{{canonical_schema}}", CANONICAL_CHAT_SCHEMA),
                ("{{task}}", request.task.as_str()),
                ("{{language}}", request.language.as_str()),
            ],
        ),
    }
}

fn render_template(source: &str, replacements: &[(&str, &str)]) -> String {
    let mut rendered = source.to_string();
    for (placeholder, value) in replacements {
        rendered = rendered.replace(placeholder, value);
    }
    rendered
}
