//! Runtime profile metadata for model-bound server starts.

use crate::features::server::domain::{
    ServerCapability, ServerRuntimeBackend, ServerRuntimeProfileSelection,
};

pub const LOCAL_CHAT_TRANSFORMERS_PEFT_PROFILE_ID: &str = "local-chat-transformers-peft";
pub const LOCAL_CHAT_MLX_PROFILE_ID: &str = "local-chat-mlx";
pub const LOCAL_CHAT_LLAMA_CPP_PROFILE_ID: &str = "local-chat-llama-cpp";
pub const LOCAL_CHAT_RUNTIME_PROFILE_VERSION: u32 = 1;
pub const LOCAL_EMBEDDING_TRANSFORMERS_PEFT_PROFILE_ID: &str = "local-embedding-transformers-peft";
pub const LOCAL_EMBEDDING_LLAMA_CPP_PROFILE_ID: &str = "local-embedding-llama-cpp";
pub const LOCAL_EMBEDDING_RUNTIME_PROFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRuntimeProfile {
    pub selection: ServerRuntimeProfileSelection,
    pub capability: ServerCapability,
    pub backend: ServerRuntimeBackend,
    pub accepted_parameters: &'static [&'static str],
    pub rejected_parameters: &'static [&'static str],
    pub default_context_tokens: Option<u32>,
    pub default_max_output_tokens: Option<u32>,
    pub backend_knobs: &'static [ServerRuntimeProfileKnob],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerRuntimeProfileKnob {
    pub name: &'static str,
    pub value: &'static str,
}

pub fn local_server_runtime_profile_for(
    capability: ServerCapability,
    backend: ServerRuntimeBackend,
) -> Option<ServerRuntimeProfile> {
    match (capability, backend) {
        (ServerCapability::Chat, ServerRuntimeBackend::TransformersPeft) => {
            Some(local_chat_profile(
                LOCAL_CHAT_TRANSFORMERS_PEFT_PROFILE_ID,
                backend,
                &[ServerRuntimeProfileKnob {
                    name: "runtime_family",
                    value: "transformers-peft",
                }],
            ))
        }
        (ServerCapability::Chat, ServerRuntimeBackend::Mlx) => Some(local_chat_profile(
            LOCAL_CHAT_MLX_PROFILE_ID,
            backend,
            &[ServerRuntimeProfileKnob {
                name: "runtime_family",
                value: "mlx-lm",
            }],
        )),
        (ServerCapability::Chat, ServerRuntimeBackend::LlamaCpp) => Some(local_chat_profile(
            LOCAL_CHAT_LLAMA_CPP_PROFILE_ID,
            backend,
            &[ServerRuntimeProfileKnob {
                name: "runtime_family",
                value: "llama-cpp",
            }],
        )),
        (ServerCapability::Embedding, ServerRuntimeBackend::TransformersPeft) => {
            Some(local_embedding_profile(
                LOCAL_EMBEDDING_TRANSFORMERS_PEFT_PROFILE_ID,
                backend,
                &[ServerRuntimeProfileKnob {
                    name: "runtime_family",
                    value: "transformers-embedding",
                }],
            ))
        }
        (ServerCapability::Embedding, ServerRuntimeBackend::LlamaCpp) => {
            Some(local_embedding_profile(
                LOCAL_EMBEDDING_LLAMA_CPP_PROFILE_ID,
                backend,
                &[ServerRuntimeProfileKnob {
                    name: "runtime_family",
                    value: "llama-cpp-embedding",
                }],
            ))
        }
        _ => None,
    }
}

fn local_chat_profile(
    profile_id: &'static str,
    backend: ServerRuntimeBackend,
    backend_knobs: &'static [ServerRuntimeProfileKnob],
) -> ServerRuntimeProfile {
    ServerRuntimeProfile {
        selection: ServerRuntimeProfileSelection::new(
            profile_id,
            LOCAL_CHAT_RUNTIME_PROFILE_VERSION,
        ),
        capability: ServerCapability::Chat,
        backend,
        accepted_parameters: &["messages", "temperature", "max_tokens", "stream"],
        rejected_parameters: &[
            "audio",
            "modalities",
            "response_format",
            "tools",
            "tool_choice",
        ],
        default_context_tokens: None,
        default_max_output_tokens: None,
        backend_knobs,
    }
}

fn local_embedding_profile(
    profile_id: &'static str,
    backend: ServerRuntimeBackend,
    backend_knobs: &'static [ServerRuntimeProfileKnob],
) -> ServerRuntimeProfile {
    ServerRuntimeProfile {
        selection: ServerRuntimeProfileSelection::new(
            profile_id,
            LOCAL_EMBEDDING_RUNTIME_PROFILE_VERSION,
        ),
        capability: ServerCapability::Embedding,
        backend,
        accepted_parameters: &["input", "model", "encoding_format=float"],
        rejected_parameters: &["dimensions", "encoding_format=base64", "user"],
        default_context_tokens: None,
        default_max_output_tokens: None,
        backend_knobs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_chat_profiles_are_selected_by_backend() {
        let profile =
            local_server_runtime_profile_for(ServerCapability::Chat, ServerRuntimeBackend::Mlx)
                .expect("profile");

        assert_eq!(profile.selection.label(), "local-chat-mlx-v1");
        assert_eq!(profile.capability, ServerCapability::Chat);
        assert_eq!(profile.backend, ServerRuntimeBackend::Mlx);
        assert!(profile.accepted_parameters.contains(&"messages"));
    }

    #[test]
    fn non_chat_profiles_are_not_selected_in_this_slice() {
        let profile =
            local_server_runtime_profile_for(ServerCapability::Rerank, ServerRuntimeBackend::Mlx);

        assert!(profile.is_none());
    }

    #[test]
    fn local_embedding_profiles_are_selected_for_supported_backends() {
        let transformers = local_server_runtime_profile_for(
            ServerCapability::Embedding,
            ServerRuntimeBackend::TransformersPeft,
        )
        .expect("transformers profile");
        let llama_cpp = local_server_runtime_profile_for(
            ServerCapability::Embedding,
            ServerRuntimeBackend::LlamaCpp,
        )
        .expect("llama-cpp profile");

        assert_eq!(
            transformers.selection.label(),
            "local-embedding-transformers-peft-v1"
        );
        assert_eq!(llama_cpp.selection.label(), "local-embedding-llama-cpp-v1");
        assert!(transformers.accepted_parameters.contains(&"input"));
        assert!(transformers.rejected_parameters.contains(&"dimensions"));
    }

    #[test]
    fn local_embedding_profile_is_not_selected_for_mlx_yet() {
        let profile = local_server_runtime_profile_for(
            ServerCapability::Embedding,
            ServerRuntimeBackend::Mlx,
        );

        assert!(profile.is_none());
    }
}
