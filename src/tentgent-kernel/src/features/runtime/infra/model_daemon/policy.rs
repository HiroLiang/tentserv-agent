use serde::{Deserialize, Serialize};

use crate::features::{model::domain::ModelCapability, runtime::domain::ModelRuntimeIdlePolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ModelRuntimeCapability {
    #[serde(rename = "audio-speech")]
    AudioSpeech,
    #[serde(rename = "audio-transcription")]
    AudioTranscription,
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "embedding")]
    Embedding,
    #[serde(rename = "image-generation")]
    ImageGeneration,
    #[serde(rename = "lora-tuning")]
    LoraTuning,
    #[serde(rename = "rerank")]
    Rerank,
    #[serde(rename = "video-understanding")]
    VideoUnderstanding,
    #[serde(rename = "vision-chat")]
    VisionChat,
}

impl ModelRuntimeCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AudioSpeech => "audio-speech",
            Self::AudioTranscription => "audio-transcription",
            Self::Chat => "chat",
            Self::Embedding => "embedding",
            Self::ImageGeneration => "image-generation",
            Self::LoraTuning => "lora-tuning",
            Self::Rerank => "rerank",
            Self::VideoUnderstanding => "video-understanding",
            Self::VisionChat => "vision-chat",
        }
    }

    pub const fn from_model_capability(capability: ModelCapability) -> Self {
        match capability {
            ModelCapability::AudioSpeech => Self::AudioSpeech,
            ModelCapability::AudioTranscription => Self::AudioTranscription,
            ModelCapability::Chat => Self::Chat,
            ModelCapability::Embedding => Self::Embedding,
            ModelCapability::ImageGeneration => Self::ImageGeneration,
            ModelCapability::Rerank => Self::Rerank,
            ModelCapability::VideoUnderstanding => Self::VideoUnderstanding,
            ModelCapability::VisionChat => Self::VisionChat,
        }
    }
}

impl std::fmt::Display for ModelRuntimeCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRuntimeDaemonLaunchPolicy {
    pub runtime_idle_seconds: u64,
    pub model_idle_seconds: u64,
}

impl ModelRuntimeDaemonLaunchPolicy {
    pub fn from_idle_policy(policy: ModelRuntimeIdlePolicy) -> Self {
        Self {
            runtime_idle_seconds: policy.runtime_idle_seconds,
            model_idle_seconds: policy.model_idle_seconds,
        }
    }

    pub fn new(runtime_idle_seconds: u64, model_idle_seconds: u64) -> Result<Self, String> {
        ModelRuntimeIdlePolicy::new(runtime_idle_seconds, model_idle_seconds)
            .map(Self::from_idle_policy)
    }
}

impl Default for ModelRuntimeDaemonLaunchPolicy {
    fn default() -> Self {
        Self::from_idle_policy(ModelRuntimeIdlePolicy::default())
    }
}
