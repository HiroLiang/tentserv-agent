use super::{
    claude_messages::{
        claude_messages_response_value, claude_text_content, ClaudeMessage, ClaudeMessagesRequest,
    },
    embeddings::{self, embedding_response, EmbeddingRequest},
    error::CloudServerError,
    gemini_generate::{
        gemini_operation_stream, gemini_request_into_cloud, gemini_response_value,
        GeminiGenerateContentRequest,
    },
    images::ImageRequest,
    openai_chat::{openai_chat_response_value, OpenAiChatRequest, OpenAiMessage},
    CloudServerRuntimeConfig, CloudServerState,
};
use crate::provider_compat::ensure_provider_capability;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use serde_json::{json, Value};
use tentgent_kernel::{
    features::{
        auth::domain::Provider,
        cloud::domain::{CloudChatContentPart, CloudEndpointCapability},
    },
    foundation::error::KernelError,
};
use tower::ServiceExt;

mod gemini_and_embeddings;
mod openai_and_claude;
