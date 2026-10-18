use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::CompatibilityError;
use crate::features::model::domain::ModelCapability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderShape {
    Native,
    Openai,
    Claude,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Text,
    Json,
    FloatVector,
    Base64,
    RankedDocuments,
    Wav,
    Pcm,
    Mp3,
    Png,
    Jpeg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageWorkflow {
    TextToImage,
    ImageToImage,
    Inpainting,
    Controlnet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingInputKind {
    Query,
    Document,
}

/// Fixed allowlist only. Prompts, filenames, headers, sampling values, and
/// request-size buckets are deliberately not representable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeAttributes {
    pub tool_calls: bool,
    pub structured_output: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_workflow: Option<ImageWorkflow>,
    /// None means observation confirmed no query/document distinction was
    /// requested. It must never stand for an unresolved execution fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_input: Option<EmbeddingInputKind>,
}

impl ShapeAttributes {
    fn validate(&self, family: ModelCapability) -> Result<(), CompatibilityError> {
        if self.image_workflow.is_some() != (family == ModelCapability::ImageGeneration)
            || self.embedding_input.is_some() && family != ModelCapability::Embedding
            || (self.tool_calls || self.structured_output)
                && !matches!(
                    family,
                    ModelCapability::Chat
                        | ModelCapability::VisionChat
                        | ModelCapability::VideoUnderstanding
                )
        {
            return Err(CompatibilityError::InvalidField("shape_attributes"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputShape {
    pub family: ModelCapability,
    /// A set has one canonical order regardless of request ordering/duplicates.
    pub modalities: BTreeSet<Modality>,
    pub provider: ProviderShape,
    pub attributes: ShapeAttributes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputShape {
    pub family: ModelCapability,
    pub modalities: BTreeSet<Modality>,
    pub streaming: bool,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservationScope {
    Load,
    Execution,
}

/// This is evidence scope, NOT the server's lazy/eager startup load mode.
/// A preload observation cannot fabricate input/output shapes or prove inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", from = "ObservationWire")]
pub enum Observation {
    Load,
    Execution {
        input: InputShape,
        output: OutputShape,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum ObservationWire {
    Load {},
    Execution {
        input: InputShape,
        output: OutputShape,
    },
}

impl From<ObservationWire> for Observation {
    fn from(value: ObservationWire) -> Self {
        match value {
            ObservationWire::Load {} => Self::Load,
            ObservationWire::Execution { input, output } => Self::Execution { input, output },
        }
    }
}

impl Observation {
    pub const fn scope(&self) -> ObservationScope {
        match self {
            Self::Load => ObservationScope::Load,
            Self::Execution { .. } => ObservationScope::Execution,
        }
    }

    pub(super) fn validate(&self, capability: ModelCapability) -> Result<(), CompatibilityError> {
        if let Self::Execution { input, output } = self {
            if input.family != capability
                || output.family != capability
                || input.modalities.is_empty()
                || output.modalities.is_empty()
            {
                return Err(CompatibilityError::InvalidField("observation_shape"));
            }
            input.attributes.validate(capability)?;
        }
        Ok(())
    }
}
