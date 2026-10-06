"""Request-time image workflow dispatch; no real image weights are loaded."""

import asyncio
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

import pytest
from fastapi import HTTPException
from preload_fixtures import FakeModel
from tentgent.runtime.backends.image_generation import ImageGenerationResult
from tentgent.runtime.backends.records import ModelCapability, ModelFormat
from tentgent.runtime.server.app import create_app
from tentgent.runtime.server.lifecycle import RuntimeCapability, RuntimeServerConfig
from tentgent.runtime.server.routes import image_generation as routes
from test_server_bound_models import _write_model


class FakeImage(FakeModel):
    def __init__(self):
        super().__init__()
        self.workflows = []

    def generate_image(self, request):
        assert self.is_loaded
        self.workflows.append(request.workflow_kind.value)
        request.output_path.write_bytes(b"fixture image")
        return ImageGenerationResult(
            output_format=request.output_format,
            media_type="image/png",
            output_path=request.output_path,
            total_bytes=13,
            width=request.width,
            height=request.height,
            seed=request.seed,
        )


@pytest.mark.parametrize("format_", [ModelFormat.DIFFUSERS, ModelFormat.MLX])
@pytest.mark.parametrize(
    "workflow", ["text-to-image", "image-to-image", "inpaint", "control"]
)
def test_lazy_image_prepares_only_requested_workflow(
    tmp_path, monkeypatch, format_, workflow
):
    monkeypatch.delenv("TENTGENT_DATA_ROOT", raising=False)
    # Pixel decoding is not under test; keep this dispatch/lifecycle matrix
    # runnable with the base environment, without Pillow or model dependencies.
    decoded = MagicMock()
    decoded.__enter__.return_value.size = (8, 8)
    monkeypatch.setattr(
        "tentgent.runtime.backends.image_generation._load_pillow_image",
        lambda: SimpleNamespace(open=lambda _path: decoded),
    )
    model_ref = "6" * 64
    _write_model(
        tmp_path,
        model_ref=model_ref,
        format_=format_,
        capability=ModelCapability.IMAGE_GENERATION,
    )
    input_path = tmp_path / "input.png"
    input_path.write_bytes(b"fixture input")
    control_path = tmp_path / "control-model"
    control_path.mkdir()
    common = {"prompt": "fixture", "output_path": str(tmp_path / "out.png")}
    choices = {
        "text-to-image": (routes.ImageGenerationPayload, routes.image_generations, {}),
        "image-to-image": (
            routes.ImageTransformPayload,
            routes.image_transforms,
            {"input_image_path": str(input_path)},
        ),
        "inpaint": (
            routes.ImageInpaintPayload,
            routes.image_inpaint,
            {"input_image_path": str(input_path), "mask_image_path": str(input_path)},
        ),
        "control": (
            routes.ImageControlPayload,
            routes.image_control,
            {
                "control_image_path": str(input_path),
                "control": {"control_ref": "fixture", "source_path": str(control_path)},
            },
        ),
    }
    payload_type, endpoint, fields = choices[workflow]
    model = FakeImage()

    async def run():
        with patch(
            "tentgent.runtime.server.app.build_image_generation_model",
            return_value=model,
        ) as factory:
            app = create_app(
                RuntimeServerConfig(
                    host="127.0.0.1",
                    port=0,
                    home=tmp_path,
                    model_ref=model_ref,
                    capability=RuntimeCapability.IMAGE_GENERATION,
                    lazy_load=True,
                )
            )
        await app.state.lifecycle.start()
        try:
            factory.assert_not_called()
            assert model.load_count == 0
            request = SimpleNamespace(app=app)
            payload = payload_type(**common, **fields)
            if format_ == ModelFormat.MLX and workflow == "control":
                with pytest.raises(HTTPException) as raised:
                    await endpoint(payload, request)
                assert raised.value.status_code == 400
                factory.assert_not_called()
                return
            response = await endpoint(payload, request)
            assert response.status == "done"
            expected = (
                ("diffusers" if format_ == ModelFormat.DIFFUSERS else "mlx-diffusion")
                + "-"
                + workflow
            )
            assert [call.args[0].value for call in factory.call_args_list] == [expected]
            assert model.workflows == [workflow]
            assert model.load_count == model.release_count == 1
            assert app.state.resource_manager.snapshot()["model_resource_count"] == 0
        finally:
            await app.state.lifecycle.stop()

    asyncio.run(run())
