use super::domain::{
    ChatBackend, ChatFinishReason, ChatMessage, ChatPrompt, ChatRole, ChatRoleParseError,
};
use crate::features::adapter::domain::AdapterBackendSupport;
use crate::features::model::domain::ModelFormat;

#[test]
fn chat_role_parses_supported_roles() {
    assert_eq!(ChatRole::parse(" system "), Ok(ChatRole::System));
    assert_eq!(ChatRole::parse("USER"), Ok(ChatRole::User));
    assert_eq!(ChatRole::parse("assistant"), Ok(ChatRole::Assistant));
    assert_eq!(ChatRole::parse(""), Err(ChatRoleParseError::Empty));
}

#[test]
fn chat_message_trims_and_rejects_empty_content() {
    let message = ChatMessage::user(" hello ").expect("message");
    assert_eq!(message.role, ChatRole::User);
    assert_eq!(message.content, "hello");

    assert!(ChatMessage::assistant("  ").is_err());
}

#[test]
fn chat_prompt_requires_messages() {
    assert_eq!(
        ChatPrompt::new(vec![ChatMessage::user("hi").expect("message")])
            .expect("prompt")
            .len(),
        1
    );
    assert!(ChatPrompt::new(vec![]).is_err());
}

#[test]
fn chat_backend_maps_to_adapter_backend_support() {
    assert_eq!(
        ChatBackend::from_model_format(ModelFormat::Safetensors),
        Some(ChatBackend::TransformersPeft)
    );
    assert_eq!(
        ChatBackend::from_model_format(ModelFormat::Mlx),
        Some(ChatBackend::Mlx)
    );
    assert_eq!(
        ChatBackend::from_model_format(ModelFormat::Gguf),
        Some(ChatBackend::LlamaCpp)
    );
    assert_eq!(ChatBackend::from_model_format(ModelFormat::Diffusers), None);
    assert_eq!(
        ChatBackend::TransformersPeft.adapter_backend_support(),
        AdapterBackendSupport::TransformersPeft
    );
    assert_eq!(
        ChatBackend::Mlx.adapter_backend_support(),
        AdapterBackendSupport::Mlx
    );
    assert_eq!(
        ChatBackend::LlamaCpp.adapter_backend_support(),
        AdapterBackendSupport::LlamaCpp
    );
}

#[test]
fn finish_reason_exposes_contract_string() {
    assert_eq!(ChatFinishReason::Stop.as_str(), "stop");
    assert_eq!(
        ChatFinishReason::Other("tool_calls".to_string()).as_str(),
        "tool_calls"
    );
}
