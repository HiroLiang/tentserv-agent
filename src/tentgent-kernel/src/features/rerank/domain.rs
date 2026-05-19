//! Rerank request, target, response, and backend domain types.

use serde::{Deserialize, Serialize};

use crate::features::model::domain::{ModelCapability, ModelFormat, ModelRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankInput {
    pub query: String,
    pub documents: Vec<String>,
    pub top_n: Option<usize>,
}

impl RerankInput {
    pub fn new(
        query: String,
        documents: Vec<String>,
        top_n: Option<usize>,
    ) -> Result<Self, RerankInputValidationError> {
        if query.trim().is_empty() {
            return Err(RerankInputValidationError::EmptyQuery);
        }
        if documents.is_empty() {
            return Err(RerankInputValidationError::EmptyDocuments);
        }
        if documents.iter().any(|document| document.trim().is_empty()) {
            return Err(RerankInputValidationError::EmptyDocument);
        }
        if let Some(limit) = top_n {
            if limit == 0 || limit > documents.len() {
                return Err(RerankInputValidationError::InvalidTopN {
                    top_n: limit,
                    document_count: documents.len(),
                });
            }
        }

        Ok(Self {
            query,
            documents,
            top_n,
        })
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RerankInputValidationError {
    #[error("rerank query must not be empty")]
    EmptyQuery,
    #[error("rerank documents must contain at least one string")]
    EmptyDocuments,
    #[error("rerank documents must not be empty")]
    EmptyDocument,
    #[error("rerank top_n must be between 1 and document count {document_count}; got {top_n}")]
    InvalidTopN { top_n: usize, document_count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RerankBackend {
    TransformersSequenceClassification,
}

impl RerankBackend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TransformersSequenceClassification => "transformers-sequence-classification",
        }
    }

    pub const fn from_model_format(format: ModelFormat) -> Option<Self> {
        match format {
            ModelFormat::Safetensors => Some(Self::TransformersSequenceClassification),
            ModelFormat::Gguf | ModelFormat::Mlx => None,
        }
    }
}

impl std::fmt::Display for RerankBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RerankRuntimeTarget {
    LocalModel {
        model_ref: ModelRef,
        backend: RerankBackend,
        source_repo: Option<String>,
        source_revision: Option<String>,
        model_capabilities: Vec<ModelCapability>,
    },
}

impl RerankRuntimeTarget {
    pub fn model_label(&self) -> String {
        match self {
            Self::LocalModel { model_ref, .. } => model_ref.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRerankTarget {
    pub runtime: RerankRuntimeTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankRequest {
    pub target: ResolvedRerankTarget,
    pub input: RerankInput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RerankResponse {
    pub data: Vec<RerankScore>,
}

impl RerankResponse {
    pub fn ranked_from_scores(scores: Vec<f32>, top_n: Option<usize>) -> Self {
        let mut data = scores
            .into_iter()
            .enumerate()
            .map(|(index, score)| RerankScore { index, score })
            .collect::<Vec<_>>();
        data.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.index.cmp(&right.index))
        });
        if let Some(limit) = top_n {
            data.truncate(limit);
        }
        Self { data }
    }
}
