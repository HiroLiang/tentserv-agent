use serde::{Deserialize, Serialize};

use crate::features::model::domain::ModelCapability;

const IDLE_KEEP_ALIVE_SECONDS: &str = "300";
const MODEL_IDLE_TIMEOUT_SECONDS: &str = "-1";

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
    pub idle_keep_alive_seconds: String,
    pub model_idle_timeout_seconds: String,
}

impl ModelRuntimeDaemonLaunchPolicy {
    pub fn with_idle_keep_alive_seconds(seconds: u64) -> Self {
        Self {
            idle_keep_alive_seconds: seconds.to_string(),
            model_idle_timeout_seconds: MODEL_IDLE_TIMEOUT_SECONDS.to_string(),
        }
    }
}

impl Default for ModelRuntimeDaemonLaunchPolicy {
    fn default() -> Self {
        Self {
            idle_keep_alive_seconds: IDLE_KEEP_ALIVE_SECONDS.to_string(),
            model_idle_timeout_seconds: MODEL_IDLE_TIMEOUT_SECONDS.to_string(),
        }
    }
}
