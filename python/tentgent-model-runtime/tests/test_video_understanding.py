from __future__ import annotations

import tempfile
import unittest
from contextlib import nullcontext
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from tentgent.runtime.backends.records import ModelFormat, ModelRecord
from tentgent.runtime.backends.video_understanding import (
    MLX_VIDEO_SUPPORTED_MODEL_TYPES,
    UnsupportedMlxVideoModelError,
    VideoFocusRegion,
    VideoSamplingOptions,
    VideoUnderstandingContext,
    VideoUnderstandingModelKind,
    VideoUnderstandingOutputFormat,
    VideoUnderstandingRequest,
    detect_model_type,
    ensure_mlx_video_model_supported,
    normalize_video_understanding_request,
    render_video_prompt_text,
)
from tentgent.runtime.backends.transformers.video_understanding import (
    TransformersVideoUnderstandingModel,
)
from tentgent.runtime.backends.mlx.video_understanding import (
    MlxVlmVideoUnderstandingModel,
)


class VideoUnderstandingContractTests(unittest.TestCase):
    def test_normalizes_request_focus_context_and_sampling(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            video_path = Path(tmp) / "clip.mp4"
            video_path.write_bytes(b"video")

            request = normalize_video_understanding_request(
                VideoUnderstandingRequest(
                    video_path=video_path,
                    prompt="  What changes near the door? ",
                    system_prompt="  answer briefly ",
                    output_format="markdown",
                    sampling=VideoSamplingOptions(
                        sample_fps=0.5,
                        max_frames=4,
                        max_frame_edge=256,
                        clip_start_seconds=1.0,
                        clip_duration_seconds=3.0,
                    ),
                    focus_regions=(
                        VideoFocusRegion(
                            x=0.5,
                            y=0.1,
                            width=0.25,
                            height=0.4,
                            label=" door ",
                        ),
                    ),
                    context=VideoUnderstandingContext(
                        transcript="  hello there ",
                        notes=(" fixed camera ", " "),
                    ),
                )
            )

        self.assertEqual(request.output_format, VideoUnderstandingOutputFormat.MD)
        self.assertEqual(request.prompt, "What changes near the door?")
        self.assertEqual(request.system_prompt, "answer briefly")
        self.assertEqual(request.sampling.sample_fps, 0.5)
        self.assertEqual(request.focus_regions[0].label, "door")
        self.assertEqual(request.context.transcript, "hello there")
        self.assertEqual(request.context.notes, ("fixed camera",))

        prompt = render_video_prompt_text(request)
        self.assertIn("Focus regions", prompt)
        self.assertIn("Transcript:", prompt)
        self.assertIn("What changes near the door?", prompt)

    def test_mlx_model_type_detection_and_supported_error_detail(self) -> None:
        self.assertEqual(
            detect_model_type({"architectures": ["Qwen2VLForConditionalGeneration"]}),
            "qwen2_vl",
        )
        self.assertEqual(
            ensure_mlx_video_model_supported(SimpleNamespace(model_type="qwen2_5_vl")),
            "qwen2_5_vl",
        )

        with self.assertRaises(UnsupportedMlxVideoModelError) as raised:
            ensure_mlx_video_model_supported(SimpleNamespace(model_type="gemma3"))

        detail = raised.exception.to_http_detail()
        self.assertEqual(detail["error"], "mlx_video_model_unsupported")
        self.assertEqual(detail["model_type"], "gemma3")
        self.assertEqual(
            detail["supported_model_types"],
            list(MLX_VIDEO_SUPPORTED_MODEL_TYPES),
        )


class TransformersVideoUnderstandingTests(unittest.TestCase):
    def test_transformers_backend_uses_sampled_frames_and_prompt_hints(self) -> None:
        model = object.__new__(TransformersVideoUnderstandingModel)
        model._deps = SimpleNamespace(torch=FakeTorch())
        model._record = _model_record(ModelFormat.SAFETENSORS)
        model._processor = FakeTransformersProcessor()
        model._model = FakeTransformersModel()
        model._device = "cpu"
        request = VideoUnderstandingRequest(
            video_path=Path("clip.mp4"),
            prompt="Describe the action.",
            output_format=VideoUnderstandingOutputFormat.TEXT,
            system_prompt="Be precise.",
            max_tokens=12,
            temperature=0.0,
            focus_regions=(
                VideoFocusRegion(x=0.0, y=0.0, width=0.5, height=0.5, label="top"),
            ),
        )

        with patch(
            "tentgent.runtime.backends.transformers.video_understanding._sample_video_frames",
            return_value=["frame-1", "frame-2"],
        ):
            result = model.understand_video(request)

        self.assertEqual(result.text, "transformers answer")
        self.assertEqual(result.sampled_frames, 2)
        self.assertIn("Be precise.", str(model._processor.messages))
        self.assertIn("Focus regions", str(model._processor.messages))
        self.assertEqual(model._processor.images, ["frame-1", "frame-2"])
        self.assertEqual(model._model.kwargs["max_new_tokens"], 12)


class MlxVideoUnderstandingTests(unittest.TestCase):
    def test_mlx_backend_rejects_unsupported_video_model_type(self) -> None:
        model = object.__new__(MlxVlmVideoUnderstandingModel)
        model._deps = FakeMlxDeps(model_type="gemma3")
        model._record = None
        model._model = None
        model._processor = None
        model._config = None
        model._model_type = None

        with self.assertRaises(UnsupportedMlxVideoModelError):
            model.load(_model_record(ModelFormat.MLX))

    def test_mlx_backend_generates_with_native_video_inputs(self) -> None:
        model = object.__new__(MlxVlmVideoUnderstandingModel)
        model._deps = FakeMlxDeps(model_type="qwen2_vl")
        model._record = None
        model._model = None
        model._processor = None
        model._config = None
        model._model_type = None
        model.load(_model_record(ModelFormat.MLX))

        request = VideoUnderstandingRequest(
            video_path=Path("clip.mp4"),
            prompt="What happens?",
            output_format=VideoUnderstandingOutputFormat.JSON,
            max_tokens=20,
            temperature=0.1,
            sampling=VideoSamplingOptions(sample_fps=2.0, max_frames=6),
        )

        result = model.understand_video(request)

        self.assertEqual(result.text, "mlx answer")
        self.assertEqual(result.sampled_frames, 3)
        self.assertEqual(result.media_type, "application/json")
        self.assertEqual(model._deps.video_module.generate_kwargs["max_tokens"], 20)
        self.assertEqual(model._deps.video_module.messages[0]["content"][0]["fps"], 2.0)
        self.assertEqual(
            model._deps.video_module.messages[0]["content"][0]["max_frames"],
            6,
        )


class VideoUnderstandingRouteTests(unittest.TestCase):
    def test_route_builds_inference_request(self) -> None:
        from tentgent.runtime.server.routes.payloads import ModelRecordPayload
        from tentgent.runtime.server.lifecycle import RuntimeCapability
        from tentgent.runtime.server.routes.video_understanding import (
            VideoSamplingPayload,
            VideoUnderstandingPayload,
            _build_video_understanding_inference_request,
        )

        with tempfile.TemporaryDirectory() as tmp:
            video_path = Path(tmp) / "clip.mp4"
            video_path.write_bytes(b"video")
            payload = VideoUnderstandingPayload(
                model_kind=VideoUnderstandingModelKind.TRANSFORMERS_VIDEO_UNDERSTANDING,
                model=ModelRecordPayload(
                    model_ref="model-ref",
                    source_path=str(Path(tmp) / "model"),
                    primary_format=ModelFormat.SAFETENSORS,
                ),
                video_path=str(video_path),
                prompt="Describe it.",
                output_format="txt",
                sampling=VideoSamplingPayload(max_frames=2),
            )
            task_ref, inference_request = _build_video_understanding_inference_request(
                payload,
                _direct_request(Path(tmp), RuntimeCapability.VIDEO_UNDERSTANDING),
            )

        self.assertTrue(task_ref)
        self.assertEqual(
            inference_request.model_kind,
            VideoUnderstandingModelKind.TRANSFORMERS_VIDEO_UNDERSTANDING,
        )
        self.assertEqual(inference_request.video.sampling.max_frames, 2)


def _model_record(format_: ModelFormat) -> ModelRecord:
    return ModelRecord(
        model_ref="model-ref",
        source_path=Path("/tmp/model"),
        primary_format=format_,
    )


def _direct_request(home: Path, capability: object) -> SimpleNamespace:
    from tentgent.runtime.server.lifecycle import RuntimeServerConfig

    return SimpleNamespace(
        app=SimpleNamespace(
            state=SimpleNamespace(
                runtime_config=RuntimeServerConfig(
                    host="127.0.0.1",
                    port=8799,
                    capability=capability,
                    model_ref=None,
                    home=home,
                )
            )
        )
    )


class FakeTensor:
    shape = (1, 5)

    def to(self, _device: object) -> "FakeTensor":
        return self

    def __getitem__(self, _key: object) -> "FakeTensor":
        return self


class FakeTorch:
    @staticmethod
    def inference_mode() -> object:
        return nullcontext()

    class cuda:
        @staticmethod
        def is_available() -> bool:
            return False

        @staticmethod
        def empty_cache() -> None:
            return None

    class backends:
        class mps:
            @staticmethod
            def is_available() -> bool:
                return False

    class mps:
        @staticmethod
        def empty_cache() -> None:
            return None


class FakeTransformersProcessor:
    tokenizer = SimpleNamespace(pad_token_id=0, eos_token_id=1)

    def __call__(
        self,
        *,
        text: str,
        images: list[str],
        return_tensors: str,
    ) -> dict[str, FakeTensor]:
        self.text = text
        self.images = images
        self.return_tensors = return_tensors
        return {"input_ids": FakeTensor(), "attention_mask": FakeTensor()}

    def apply_chat_template(
        self,
        messages: list[dict[str, object]],
        *,
        tokenize: bool,
        add_generation_prompt: bool,
    ) -> str:
        self.messages = messages
        self.tokenize = tokenize
        self.add_generation_prompt = add_generation_prompt
        return "rendered prompt"

    def batch_decode(self, _ids: object, *, skip_special_tokens: bool) -> list[str]:
        self.skip_special_tokens = skip_special_tokens
        return ["transformers answer"]


class FakeTransformersModel:
    def generate(self, **kwargs: object) -> FakeTensor:
        self.kwargs = kwargs
        return FakeTensor()


class FakeMlxDeps:
    def __init__(self, *, model_type: str) -> None:
        self.mx = SimpleNamespace(array=lambda value: ("mx", value))
        self.video_module = FakeMlxVideoModule()
        self._model = SimpleNamespace(config=SimpleNamespace(model_type=model_type))

    def load(self, _path: str) -> tuple[object, object]:
        return self._model, FakeMlxProcessor()

    @staticmethod
    def load_config(_path: str) -> object:
        return SimpleNamespace(model_type="qwen2_vl")


class FakeMlxProcessor:
    chat_template = "template"

    def apply_chat_template(
        self,
        messages: list[dict[str, object]],
        *,
        tokenize: bool,
        add_generation_prompt: bool,
    ) -> str:
        self.messages = messages
        return "mlx rendered prompt"

    def __call__(self, **kwargs: object) -> dict[str, object]:
        self.kwargs = kwargs
        return {
            "input_ids": [[1, 2, 3]],
            "attention_mask": [[1, 1, 1]],
            "pixel_values_videos": FakeVideoArray(),
            "video_grid_thw": [[3, 1, 1]],
        }


class FakeVideoArray:
    shape = (3, 3, 224, 224)


class FakeMlxVideoModule:
    def process_vision_info(
        self,
        messages: list[dict[str, object]],
        _return_video_kwargs: bool,
    ) -> tuple[None, list[FakeVideoArray], dict[str, list[float]]]:
        self.messages = messages
        return None, [FakeVideoArray()], {"fps": [1.0]}

    def generate(self, *_args: object, **kwargs: object) -> object:
        self.generate_kwargs = kwargs
        return SimpleNamespace(text="mlx answer <end_of_utterance>")


if __name__ == "__main__":
    unittest.main()
