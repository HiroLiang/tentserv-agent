use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tentgent_kernel::features::{
    auth::domain::Provider,
    cloud::{
        domain::{CloudEmbeddingRequest, CloudEndpointCapability},
        infra::ReqwestCloudModelClient,
    },
};

use crate::provider_compat::{ensure_provider_capability, ProviderCompatRejection};

use super::{error::CloudServerError, CloudServerState};

pub(super) async fn embeddings(
    State(state): State<CloudServerState>,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Json<EmbeddingResponseBody>, CloudServerError> {
    request.validate()?;
    ensure_provider_capability(state.config.provider, CloudEndpointCapability::Embedding)?;
    let client = ReqwestCloudModelClient::new()?;
    let response = client
        .create_embedding(
            CloudEmbeddingRequest {
                provider: state.config.provider,
                model: state.config.provider_model.clone(),
                input: request.input.into_items(),
            },
            &state.secret,
        )
        .await?;
    Ok(Json(embedding_response(
        state.config.provider,
        state.config.provider_model,
        response.vectors,
    )))
}

#[derive(Debug, Deserialize)]
pub(super) struct EmbeddingRequest {
    input: EmbeddingInput,
    dimensions: Option<Value>,
    encoding_format: Option<Value>,
    user: Option<Value>,
}

impl EmbeddingRequest {
    pub(super) fn validate(&self) -> Result<(), CloudServerError> {
        self.reject_unsupported()?;
        self.input.validate()?;
        Ok(())
    }

    pub(super) fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.dimensions.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible embeddings do not support dimensions overrides yet",
            ));
        }
        if let Some(format) = &self.encoding_format {
            match format.as_str() {
                Some("float") => {}
                Some("base64") => {
                    return Err(ProviderCompatRejection::unsupported_field(
                        "provider-compatible embeddings do not support base64 encoding yet",
                    ))
                }
                _ => {
                    return Err(ProviderCompatRejection::unsupported_field(
                        "provider-compatible embeddings only support encoding_format `float`",
                    ))
                }
            }
        }
        if self.user.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "provider-compatible embeddings do not support user tracking metadata",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum EmbeddingInput {
    One(String),
    Many(Vec<String>),
}

impl EmbeddingInput {
    fn into_items(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }

    pub(super) fn validate(&self) -> Result<(), CloudServerError> {
        let items = match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values.as_slice(),
        };
        if items.is_empty() {
            return Err(CloudServerError::bad_request(
                "embedding input must contain at least one string",
            ));
        }
        if items.iter().any(|item| item.trim().is_empty()) {
            return Err(CloudServerError::bad_request(
                "embedding input strings must not be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum EmbeddingResponseBody {
    Native(NativeEmbeddingResponse),
    OpenAi(OpenAiEmbeddingResponse),
}

#[derive(Debug, Serialize)]
pub(super) struct NativeEmbeddingResponse {
    model_ref: String,
    data: Vec<EmbeddingItem>,
}

#[derive(Debug, Serialize)]
pub(super) struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAiEmbeddingResponse {
    object: &'static str,
    data: Vec<OpenAiEmbeddingItem>,
    model: String,
    usage: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAiEmbeddingItem {
    object: &'static str,
    index: usize,
    embedding: Vec<f32>,
}

pub(super) fn embedding_response(
    provider: Provider,
    model_ref: String,
    vectors: Vec<Vec<f32>>,
) -> EmbeddingResponseBody {
    match provider {
        Provider::OpenAI => {
            EmbeddingResponseBody::OpenAi(openai_embedding_response(model_ref, vectors))
        }
        _ => EmbeddingResponseBody::Native(native_embedding_response(model_ref, vectors)),
    }
}

fn native_embedding_response(model_ref: String, vectors: Vec<Vec<f32>>) -> NativeEmbeddingResponse {
    NativeEmbeddingResponse {
        model_ref,
        data: vectors
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingItem { index, embedding })
            .collect(),
    }
}

fn openai_embedding_response(model: String, vectors: Vec<Vec<f32>>) -> OpenAiEmbeddingResponse {
    OpenAiEmbeddingResponse {
        object: "list",
        data: vectors
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| OpenAiEmbeddingItem {
                object: "embedding",
                index,
                embedding,
            })
            .collect(),
        model,
        usage: None,
    }
}
