//! Model capability classification from external metadata evidence.

use super::domain::{HfModelMetadata, ModelCapability, ModelCapabilityAssignment};

pub fn classify_hf_model_capability(
    metadata: &HfModelMetadata,
) -> Option<ModelCapabilityAssignment> {
    let embedding_reason = embedding_reason(metadata);
    let rerank_reason = rerank_reason(metadata);
    let chat_reason = chat_reason(metadata);
    let signal_count = usize::from(embedding_reason.is_some())
        + usize::from(rerank_reason.is_some())
        + usize::from(chat_reason.is_some());

    if signal_count != 1 {
        return None;
    }

    if let Some(reason) = embedding_reason {
        return Some(ModelCapabilityAssignment::huggingface_metadata(
            ModelCapability::Embedding,
            reason,
        ));
    }
    if let Some(reason) = rerank_reason {
        return Some(ModelCapabilityAssignment::huggingface_metadata(
            ModelCapability::Rerank,
            reason,
        ));
    }
    chat_reason.map(|reason| {
        ModelCapabilityAssignment::huggingface_metadata(ModelCapability::Chat, reason)
    })
}

fn embedding_reason(metadata: &HfModelMetadata) -> Option<String> {
    if pipeline_is(metadata, &["feature-extraction", "sentence-similarity"]) {
        return Some("Hugging Face pipeline tag identifies embedding use".to_string());
    }
    if metadata.sentence_bert_config {
        return Some("snapshot contains sentence_bert_config.json".to_string());
    }
    if has_token(metadata, "sentence-transformers") {
        return Some("Hugging Face metadata identifies sentence-transformers".to_string());
    }

    None
}

fn rerank_reason(metadata: &HfModelMetadata) -> Option<String> {
    if pipeline_is(metadata, &["text-ranking"]) {
        return Some("Hugging Face pipeline tag identifies text ranking".to_string());
    }
    if has_any_token(metadata, &["reranker", "rerank", "cross-encoder"]) {
        return Some("Hugging Face tags identify reranking".to_string());
    }
    if has_sequence_classification_architecture(metadata)
        && has_any_token(metadata, &["ranking", "ranker", "reranker", "rerank"])
    {
        return Some(
            "sequence-classification architecture is paired with ranking metadata".to_string(),
        );
    }

    None
}

fn chat_reason(metadata: &HfModelMetadata) -> Option<String> {
    if pipeline_is(metadata, &["text-generation", "conversational"]) {
        return Some("Hugging Face pipeline tag identifies chat or text generation".to_string());
    }
    if metadata.tokenizer_chat_template {
        return Some("tokenizer_config.json contains a chat_template".to_string());
    }

    None
}

fn pipeline_is(metadata: &HfModelMetadata, expected: &[&str]) -> bool {
    metadata
        .pipeline_tag
        .as_deref()
        .map(normalize)
        .is_some_and(|tag| expected.iter().any(|value| tag == *value))
}

fn has_any_token(metadata: &HfModelMetadata, needles: &[&str]) -> bool {
    needles.iter().any(|needle| has_token(metadata, needle))
}

fn has_token(metadata: &HfModelMetadata, needle: &str) -> bool {
    let needle = normalize(needle);
    metadata
        .tags
        .iter()
        .chain(metadata.library_name.iter())
        .any(|value| normalize(value).contains(&needle))
}

fn has_sequence_classification_architecture(metadata: &HfModelMetadata) -> bool {
    metadata
        .config_architectures
        .iter()
        .any(|value| normalize(value).contains("sequenceclassification"))
}

fn normalize(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::model::domain::ModelCapabilitySource;

    #[test]
    fn classifies_embedding_from_sentence_transformers_metadata() {
        let assignment = classify_hf_model_capability(&HfModelMetadata {
            pipeline_tag: Some("sentence-similarity".to_string()),
            tags: vec!["sentence-transformers".to_string()],
            ..HfModelMetadata::default()
        })
        .expect("assignment");

        assert_eq!(assignment.capabilities, vec![ModelCapability::Embedding]);
        assert_eq!(
            assignment.source,
            ModelCapabilitySource::HuggingFaceMetadata
        );
    }

    #[test]
    fn classifies_rerank_from_cross_encoder_metadata() {
        let assignment = classify_hf_model_capability(&HfModelMetadata {
            tags: vec!["cross-encoder".to_string()],
            config_architectures: vec!["BertForSequenceClassification".to_string()],
            ..HfModelMetadata::default()
        })
        .expect("assignment");

        assert_eq!(assignment.capabilities, vec![ModelCapability::Rerank]);
        assert_eq!(
            assignment.source,
            ModelCapabilitySource::HuggingFaceMetadata
        );
    }

    #[test]
    fn classifies_chat_from_chat_template() {
        let assignment = classify_hf_model_capability(&HfModelMetadata {
            tokenizer_chat_template: true,
            ..HfModelMetadata::default()
        })
        .expect("assignment");

        assert_eq!(assignment.capabilities, vec![ModelCapability::Chat]);
        assert_eq!(
            assignment.source,
            ModelCapabilitySource::HuggingFaceMetadata
        );
    }

    #[test]
    fn rejects_conflicting_or_weak_huggingface_metadata() {
        assert!(classify_hf_model_capability(&HfModelMetadata {
            pipeline_tag: Some("text-generation".to_string()),
            tags: vec!["sentence-transformers".to_string()],
            ..HfModelMetadata::default()
        })
        .is_none());

        assert!(classify_hf_model_capability(&HfModelMetadata {
            config_architectures: vec!["BertForSequenceClassification".to_string()],
            ..HfModelMetadata::default()
        })
        .is_none());
    }

    #[test]
    fn does_not_infer_media_capabilities_from_huggingface_metadata_yet() {
        for pipeline_tag in [
            "automatic-speech-recognition",
            "text-to-speech",
            "image-to-text",
            "text-to-image",
        ] {
            assert!(
                classify_hf_model_capability(&HfModelMetadata {
                    pipeline_tag: Some(pipeline_tag.to_string()),
                    ..HfModelMetadata::default()
                })
                .is_none(),
                "media pipeline tag `{pipeline_tag}` should require explicit user capability"
            );
        }
    }
}
