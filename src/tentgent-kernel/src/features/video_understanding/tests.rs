use super::domain::{
    VideoSamplingOptions, VideoSamplingOptionsValidationError, VideoUnderstandingBackend,
    VideoUnderstandingOutputFormat,
};
use crate::features::model::domain::{MlxRuntimeFamily, ModelFormat};

#[test]
fn parses_video_understanding_output_format_aliases() {
    assert_eq!(
        "txt"
            .parse::<VideoUnderstandingOutputFormat>()
            .expect("txt"),
        VideoUnderstandingOutputFormat::Text
    );
    assert_eq!(
        "markdown"
            .parse::<VideoUnderstandingOutputFormat>()
            .expect("markdown"),
        VideoUnderstandingOutputFormat::Md
    );
}

#[test]
fn validates_video_sampling_bounds() {
    let invalid = VideoSamplingOptions {
        sample_fps: Some(0.0),
        ..VideoSamplingOptions::default()
    };

    assert!(matches!(
        invalid.validate().expect_err("invalid sample fps"),
        VideoSamplingOptionsValidationError::SampleFps(0.0)
    ));
}

#[test]
fn resolves_video_understanding_backend_from_supported_model_formats() {
    assert_eq!(
        VideoUnderstandingBackend::from_model_format_and_mlx_family(ModelFormat::Safetensors, None,),
        Some(VideoUnderstandingBackend::TransformersVideoUnderstanding)
    );
    assert_eq!(
        VideoUnderstandingBackend::from_model_format_and_mlx_family(
            ModelFormat::Mlx,
            Some(MlxRuntimeFamily::Vlm),
        ),
        Some(VideoUnderstandingBackend::MlxVlm)
    );
    assert_eq!(
        VideoUnderstandingBackend::from_model_format_and_mlx_family(
            ModelFormat::Mlx,
            Some(MlxRuntimeFamily::Audio),
        ),
        None
    );
}
