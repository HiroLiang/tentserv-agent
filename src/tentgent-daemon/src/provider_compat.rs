use serde::Deserialize;
use serde_json::Value;
use tentgent_kernel::{
    features::{
        auth::domain::Provider,
        cloud::domain::{provider_supports, CloudEndpointCapability},
    },
    foundation::error::KernelError,
};

use crate::transport::rest::error::RestError;

pub(crate) const UNSUPPORTED_PROVIDER_FIELD: &str = "unsupported_provider_field";
pub(crate) const UNSUPPORTED_PROVIDER_CONTENT: &str = "unsupported_provider_content";
pub(crate) const UNSUPPORTED_PROVIDER_OPERATION: &str = "unsupported_provider_operation";
pub(crate) const UNSUPPORTED_PROVIDER_CAPABILITY: &str = "unsupported_provider_capability";

#[derive(Debug, Deserialize, Default)]
pub(crate) struct OpenAiChatCompatFields {
    max_completion_tokens: Option<u32>,
    n: Option<u32>,
    stream_options: Option<OpenAiStreamOptions>,
    top_p: Option<f32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    stop: Option<Value>,
    logit_bias: Option<Value>,
    logprobs: Option<bool>,
    top_logprobs: Option<u32>,
    tools: Option<Value>,
    tool_choice: Option<Value>,
    functions: Option<Value>,
    function_call: Option<Value>,
    parallel_tool_calls: Option<bool>,
    response_format: Option<Value>,
    modalities: Option<Vec<String>>,
    audio: Option<Value>,
    prediction: Option<Value>,
    web_search_options: Option<Value>,
    metadata: Option<Value>,
    store: Option<bool>,
    seed: Option<i64>,
    service_tier: Option<String>,
    user: Option<String>,
    safety_identifier: Option<String>,
    prompt_cache_key: Option<String>,
    prompt_cache_retention: Option<String>,
    reasoning_effort: Option<String>,
    verbosity: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct OpenAiMessageCompatFields {
    tool_calls: Option<Value>,
    function_call: Option<Value>,
    audio: Option<Value>,
    refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderChatTextMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiTextMessage {
    role: String,
    content: OpenAiTextContent,
    #[serde(flatten)]
    compat: OpenAiMessageCompatFields,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenAiTextContent {
    Text(String),
    Parts(Vec<OpenAiTextContentPart>),
}

#[derive(Debug, Deserialize)]
struct OpenAiTextContentPart {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamOptions {
    include_usage: Option<bool>,
    include_obfuscation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderCompatRejection {
    code: &'static str,
    message: String,
}

impl ProviderCompatRejection {
    pub(crate) fn unsupported_field(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_FIELD, message)
    }

    pub(crate) fn unsupported_content(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_CONTENT, message)
    }

    pub(crate) fn unsupported_operation(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_OPERATION, message)
    }

    pub(crate) fn unsupported_capability(message: impl Into<String>) -> Self {
        Self::new(UNSUPPORTED_PROVIDER_CAPABILITY, message)
    }

    pub(crate) fn into_parts(self) -> (&'static str, String) {
        (self.code, self.message)
    }

    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl OpenAiChatCompatFields {
    pub(crate) fn max_completion_tokens(&self) -> Option<u32> {
        self.max_completion_tokens
    }

    pub(crate) fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tools.is_some()
            || self.functions.is_some()
            || self
                .tool_choice
                .as_ref()
                .is_some_and(|value| !openai_no_tool_choice(value))
            || self
                .function_call
                .as_ref()
                .is_some_and(|value| !openai_no_tool_choice(value))
            || self.parallel_tool_calls == Some(true)
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible tools and function calling require kernel tool-call support",
            ));
        }
        if self
            .response_format
            .as_ref()
            .is_some_and(|value| !openai_text_response_format(value))
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible structured response_format is not supported by Tentgent chat compatibility yet",
            ));
        }
        if self.n.is_some_and(|n| n != 1) {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible n values greater than 1 require multi-choice response support",
            ));
        }
        if self
            .stream_options
            .as_ref()
            .is_some_and(OpenAiStreamOptions::requires_unsupported_output)
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible stream_options usage and obfuscation output are not supported yet",
            ));
        }
        if self.audio.is_some()
            || self
                .modalities
                .as_ref()
                .is_some_and(|modalities| modalities.iter().any(|value| value != "text"))
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible audio output requires kernel multimodal support",
            ));
        }
        if self.stop.is_some()
            || self.top_p.is_some()
            || self.frequency_penalty.is_some()
            || self.presence_penalty.is_some()
            || self.logit_bias.is_some()
            || self.logprobs == Some(true)
            || self.top_logprobs.is_some()
            || self.prediction.is_some()
            || self.reasoning_effort.is_some()
            || self.verbosity.is_some()
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible advanced generation controls are not supported by Tentgent chat compatibility yet",
            ));
        }
        if self.metadata.is_some()
            || self.store == Some(true)
            || self.seed.is_some()
            || self.service_tier.is_some()
            || self.user.is_some()
            || self.safety_identifier.is_some()
            || self.prompt_cache_key.is_some()
            || self.prompt_cache_retention.is_some()
        {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible stored completion, cache, safety, and service-tier fields are not supported by Tentgent chat compatibility yet",
            ));
        }
        if self.web_search_options.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible web search options require provider tool support",
            ));
        }
        Ok(())
    }
}

impl OpenAiMessageCompatFields {
    pub(crate) fn reject_unsupported(&self) -> Result<(), ProviderCompatRejection> {
        if self.tool_calls.is_some() || self.function_call.is_some() {
            return Err(ProviderCompatRejection::unsupported_field(
                "OpenAI-compatible tool call messages require kernel tool-call support",
            ));
        }
        if self.audio.is_some() || self.refusal.is_some() {
            return Err(ProviderCompatRejection::unsupported_content(
                "OpenAI-compatible assistant audio and refusal message content is not supported by Tentgent chat compatibility yet",
            ));
        }
        Ok(())
    }
}

impl OpenAiTextMessage {
    pub(crate) fn into_text_message(
        self,
    ) -> Result<ProviderChatTextMessage, ProviderCompatRejection> {
        self.compat.reject_unsupported()?;
        Ok(ProviderChatTextMessage {
            role: openai_text_role(&self.role)?,
            content: openai_text_content(self.content)?,
        })
    }
}

impl From<ProviderCompatRejection> for RestError {
    fn from(rejection: ProviderCompatRejection) -> Self {
        let (code, message) = rejection.into_parts();
        RestError::bad_request(code, message)
    }
}

pub(crate) fn ensure_provider_capability(
    provider: Provider,
    capability: CloudEndpointCapability,
) -> Result<(), ProviderCompatRejection> {
    if provider_supports(provider, capability) {
        return Ok(());
    }
    Err(ProviderCompatRejection::unsupported_capability(
        provider_capability_message(provider, capability),
    ))
}

fn openai_no_tool_choice(value: &Value) -> bool {
    value.as_str() == Some("none")
}

fn openai_text_response_format(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "text")
}

fn openai_text_role(role: &str) -> Result<String, ProviderCompatRejection> {
    match role.trim().to_ascii_lowercase().as_str() {
        "developer" | "system" => Ok("system".to_string()),
        "user" => Ok("user".to_string()),
        "assistant" => Ok("assistant".to_string()),
        "" => Err(ProviderCompatRejection::unsupported_content(
            "OpenAI-compatible message role is empty",
        )),
        other => Err(ProviderCompatRejection::unsupported_content(format!(
            "unsupported OpenAI message role `{other}`"
        ))),
    }
}

fn openai_text_content(content: OpenAiTextContent) -> Result<String, ProviderCompatRejection> {
    match content {
        OpenAiTextContent::Text(text) => Ok(text),
        OpenAiTextContent::Parts(parts) => {
            let mut text = String::new();
            for part in parts {
                if part.kind != "text" {
                    return Err(ProviderCompatRejection::unsupported_content(format!(
                        "unsupported OpenAI content part `{}`",
                        part.kind
                    )));
                }
                text.push_str(part.text.as_deref().unwrap_or_default());
            }
            Ok(text)
        }
    }
}

impl OpenAiStreamOptions {
    fn requires_unsupported_output(&self) -> bool {
        self.include_usage == Some(true) || self.include_obfuscation == Some(true)
    }
}

pub(crate) fn map_provider_kernel_error(
    fallback_code: impl Into<String>,
    error: KernelError,
) -> RestError {
    match error {
        KernelError::UnsupportedTarget(message) => {
            ProviderCompatRejection::unsupported_capability(message).into()
        }
        other => RestError::kernel(fallback_code, other),
    }
}

fn provider_capability_message(provider: Provider, capability: CloudEndpointCapability) -> String {
    format!(
        "{} does not support cloud {} through Tentgent yet",
        provider.display_name(),
        capability.as_str()
    )
}
