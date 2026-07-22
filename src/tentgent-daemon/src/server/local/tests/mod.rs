use std::sync::{Arc, Mutex};

use axum::{
    body::{to_bytes, Body},
    extract::{OriginalUri, State as AxumState},
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::Response,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tentgent_kernel::{
    features::server::domain::ServerCapability,
    foundation::{
        layout::{
            LayoutResolveMode, RuntimeLayoutInput, RuntimeLayoutResolver, StdRuntimeLayoutResolver,
        },
        net::http_url_from_host_port,
    },
};

use super::{
    claude_messages::{claude_messages_to_upstream, LocalClaudeMessagesRequest},
    gemini_generate::{gemini_generate_content_to_upstream, LocalGeminiGenerateContentRequest},
    openai_chat::{openai_chat_completions_to_upstream, LocalOpenAiChatCompletionRequest},
    openai_embeddings::{
        local_embedding_request_uses_openai_shape, native_embedding_to_upstream,
        openai_embeddings_to_upstream, LocalOpenAiEmbeddingRequest,
    },
    openai_images::{
        local_image_generation_request_uses_openai_shape, openai_image_generation_to_upstream,
        LocalOpenAiImageGenerationRequest,
    },
    proxy::{forward_to_runtime, runtime_upstream_path_and_query},
    PROXY_BODY_LIMIT_BYTES, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH, RUNTIME_EMBEDDINGS_PATH,
    RUNTIME_IMAGE_GENERATIONS_PATH,
};

mod fixtures;
mod provider_routes;
mod proxy_and_chat;

use fixtures::*;
