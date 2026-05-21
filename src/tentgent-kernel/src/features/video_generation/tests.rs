use super::domain::{
    VideoGenerationArtifactPlan, VideoGenerationDimensions, VideoGenerationInput,
    VideoGenerationOptions, VideoGenerationOptionsError, VideoGenerationOutputFormat,
    VideoGenerationPrompt,
};

#[test]
fn parses_video_generation_output_format_metadata() {
    let format = "webm".parse::<VideoGenerationOutputFormat>().expect("webm");

    assert_eq!(format, VideoGenerationOutputFormat::Webm);
    assert_eq!(VideoGenerationOutputFormat::Mp4.media_type(), "video/mp4");
    assert_eq!(
        VideoGenerationOutputFormat::Mp4.default_filename(),
        "video.mp4"
    );
    assert!("gif"
        .parse::<VideoGenerationOutputFormat>()
        .expect_err("unsupported")
        .to_string()
        .contains("unsupported video generation output format"));
}

#[test]
fn video_generation_prompt_trims_text_and_rejects_blank_prompt() {
    let prompt = VideoGenerationPrompt::new(
        " a paper boat crossing a puddle ",
        Some(" blurry ".to_string()),
    )
    .expect("prompt");

    assert_eq!(prompt.prompt, "a paper boat crossing a puddle");
    assert_eq!(prompt.negative_prompt.as_deref(), Some("blurry"));
    assert!(VideoGenerationPrompt::new(" ", None).is_err());
    assert!(VideoGenerationPrompt::new(
        "x".repeat(VideoGenerationPrompt::MAX_PROMPT_BYTES + 1),
        None
    )
    .is_err());
}

#[test]
fn video_generation_options_validate_small_fixture_bounds() {
    let dimensions = VideoGenerationDimensions::new(256, 320).expect("dimensions");
    let options =
        VideoGenerationOptions::new(dimensions, 2.5, 8, None, 12, 5.0, Some(42)).expect("options");

    assert_eq!(options.planned_frames(), 20);
    assert_eq!(options.seed, Some(42));
    assert!(VideoGenerationDimensions::new(257, 256).is_err());
    assert!(VideoGenerationOptions::new(dimensions, 0.0, 8, None, 12, 5.0, None).is_err());
    assert!(VideoGenerationOptions::new(dimensions, 2.0, 60, None, 12, 5.0, None).is_err());
    assert!(VideoGenerationOptions::new(dimensions, 2.0, 8, None, 0, 5.0, None).is_err());
    assert!(VideoGenerationOptions::new(dimensions, 2.0, 8, None, 12, f32::NAN, None).is_err());
}

#[test]
fn video_generation_options_reject_oversized_frame_plan() {
    let dimensions = VideoGenerationDimensions::default();
    let error = VideoGenerationOptions::new(dimensions, 4.0, 12, Some(121), 12, 5.0, None)
        .expect_err("frame count");

    assert!(matches!(
        error,
        VideoGenerationOptionsError::FrameCountOutOfRange { num_frames: 121 }
    ));
}

#[test]
fn video_generation_artifact_plan_defaults_to_text_to_video() {
    let plan = VideoGenerationArtifactPlan {
        input: VideoGenerationInput::default(),
        prompt: VideoGenerationPrompt::new("a candle flickering", None).expect("prompt"),
        output_path: "out.mp4".into(),
        output_format: VideoGenerationOutputFormat::default(),
        options: VideoGenerationOptions::default(),
    };

    assert_eq!(plan.input.workflow_kind().as_str(), "text-to-video");
    assert_eq!(plan.output_format, VideoGenerationOutputFormat::Mp4);
    assert_eq!(plan.options.planned_frames(), 16);
}
