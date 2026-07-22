mod capability;
pub(super) mod claude_messages;
pub(super) mod error;
mod evidence;
pub(super) mod gemini_generate;
pub(in crate::server) mod managed_adapter;
mod native;
pub(super) mod openai_chat;
pub(super) mod openai_embeddings;
mod openai_images;
pub(super) mod proxy;
mod runtime;
mod sse;

#[cfg(test)]
mod tests;

pub(super) use claude_messages::{claude_messages, LocalClaudeMessagesRequest};
pub(super) use error::LocalServerError;
pub(super) use gemini_generate::{gemini_generate_content, LocalGeminiGenerateContentRequest};
pub(super) use managed_adapter::{
    managed_native_chat, managed_native_chat_stream, ManagedNativeChatRequest,
};
pub(super) use openai_chat::{openai_chat_completions, LocalOpenAiChatCompletionRequest};
pub(super) use openai_embeddings::openai_embeddings;
use openai_images::image_generations;
pub(super) use proxy::proxy_request;
pub use runtime::{run_local_server_runtime, LocalServerRuntimeConfig};
pub(super) use runtime::{
    LocalServerState, PROXY_BODY_LIMIT_BYTES, RUNTIME_CHAT_PATH, RUNTIME_CHAT_STREAM_PATH,
    RUNTIME_EMBEDDINGS_PATH, RUNTIME_IMAGE_GENERATIONS_PATH,
};
