from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any

from ..errors import missing_backend_dependency
from ..image_generation import (
    ImageGenerationAdapterSelection,
    ImageGenerationBackendModel,
    ImageGenerationRequest,
    ImageGenerationResult,
    ImageGenerationWorkflowKind,
    load_normalized_control_image,
    load_normalized_inpaint_images,
    write_image_generation_output,
)
from ..records import ModelRecord
from .base import (
    DiffusersBackendModel,
    clear_torch_device_cache,
    controlnet_load_kwargs,
    detect_diffusers_device,
    diffusers_load_kwargs,
    require_diffusers_model,
)


@dataclass(frozen=True, slots=True)
class _DiffusersDeps:
    torch: Any
    DiffusionPipeline: Any
    AutoPipelineForImage2Image: Any
    AutoPipelineForInpainting: Any
    ControlNetModel: Any
    StableDiffusionControlNetPipeline: Any
    PILImage: Any


class DiffusersImageGenerationModel(
    DiffusersBackendModel,
    ImageGenerationBackendModel,
):
    def __init__(self) -> None:
        self._deps = _load_diffusers_deps()
        self._record: ModelRecord | None = None
        self._pipeline: Any | None = None
        self._pipeline_workflow: str | None = None
        self._device = detect_diffusers_device(self._deps.torch)
        self._adapter: ImageGenerationAdapterSelection | None = None

    def load(self, record: ModelRecord) -> None:
        require_diffusers_model(record, "Diffusers image generation model")
        self._record = record
        self._pipeline = None
        self._pipeline_workflow = None

    @property
    def is_loaded(self) -> bool:
        return self._record is not None

    def release(self) -> None:
        self._record = None
        self._pipeline = None
        self._pipeline_workflow = None
        self._adapter = None
        clear_torch_device_cache(self._deps.torch)

    def select_adapter(self, adapter: ImageGenerationAdapterSelection | None) -> None:
        if self._adapter == adapter:
            return
        self._adapter = adapter
        if self._pipeline is not None:
            self._pipeline = None
            self._pipeline_workflow = None

    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        workflow = request.workflow_kind
        pipeline = self._load_pipeline(workflow, request)
        kwargs: dict[str, object] = {
            "prompt": request.prompt,
            "width": request.width,
            "height": request.height,
            "num_inference_steps": request.steps,
            "guidance_scale": request.guidance_scale,
        }
        if request.negative_prompt:
            kwargs["negative_prompt"] = request.negative_prompt
        if workflow == ImageGenerationWorkflowKind.CONTROL:
            if request.control is None:
                raise ValueError("image control generation requires a control adapter")
            if request.control_strength is None:
                raise ValueError("image control strength is required")
            if request.control_image_path is None:
                raise ValueError("image control generation requires control_image_path")
            kwargs["image"] = load_normalized_control_image(
                request.control_image_path,
                request.width,
                request.height,
            )
            kwargs["controlnet_conditioning_scale"] = request.control_strength
        elif request.input_image_path is not None:
            if request.strength is None:
                raise ValueError("image denoising strength is required")
            if workflow == ImageGenerationWorkflowKind.INPAINT:
                if request.mask_image_path is None:
                    raise ValueError("image inpaint requires mask_image_path")
                image, mask_image = load_normalized_inpaint_images(
                    request.input_image_path,
                    request.mask_image_path,
                    request.width,
                    request.height,
                )
                kwargs["image"] = image
                kwargs["mask_image"] = mask_image
            else:
                kwargs["image"] = _load_transform_image(
                    self._deps.PILImage,
                    request.input_image_path,
                    request.width,
                    request.height,
                )
            kwargs["strength"] = request.strength
        if request.seed is not None:
            kwargs["generator"] = self._deps.torch.Generator(device="cpu").manual_seed(
                request.seed
            )
        if self._adapter is not None and not hasattr(pipeline, "set_adapters"):
            kwargs["cross_attention_kwargs"] = {"scale": self._adapter.lora_scale}

        with self._deps.torch.inference_mode():
            raw_result = pipeline(**kwargs)
        image = raw_result.images[0]
        return write_image_generation_output(request, image)

    def _load_pipeline(
        self,
        workflow: ImageGenerationWorkflowKind,
        request: ImageGenerationRequest,
    ) -> Any:
        if self._record is None:
            raise RuntimeError(
                "Diffusers image generation model is not loaded yet; call load() first."
            )
        pipeline_key = _pipeline_key_for_request(workflow, request)
        if self._pipeline is not None and self._pipeline_workflow == pipeline_key:
            return self._pipeline

        load_kwargs = diffusers_load_kwargs(
            self._record,
            self._deps.torch,
            self._device,
        )
        pipeline_class = _pipeline_class_for_workflow(self._deps, workflow)
        pipeline_kwargs: dict[str, object] = dict(load_kwargs)
        if workflow == ImageGenerationWorkflowKind.CONTROL:
            if request.control is None:
                raise ValueError("Diffusers ControlNet generation requires a control adapter")
            controlnet = self._deps.ControlNetModel.from_pretrained(
                str(request.control.source_path),
                local_files_only=True,
                **controlnet_load_kwargs(self._deps.torch, self._device),
            )
            pipeline_kwargs["controlnet"] = controlnet
        pipeline = pipeline_class.from_pretrained(
            str(self._record.source_path),
            local_files_only=True,
            **pipeline_kwargs,
        )
        pipeline.to(self._device)
        if hasattr(pipeline, "enable_attention_slicing"):
            pipeline.enable_attention_slicing()

        self._pipeline = pipeline
        self._pipeline_workflow = pipeline_key
        self._apply_selected_adapter()
        return pipeline

    def _apply_selected_adapter(self) -> None:
        if self._adapter is None:
            return
        pipeline = self._require_pipeline()
        if not hasattr(pipeline, "load_lora_weights"):
            raise RuntimeError(
                "Diffusers image generation pipeline for "
                f"`{self._record.short_ref if self._record else 'unknown'}` does not "
                "support LoRA weights."
            )
        weight_path = _adapter_weight_path(self._adapter)
        pipeline.load_lora_weights(
            str(weight_path.parent),
            weight_name=weight_path.name,
            adapter_name="tentgent",
        )
        if hasattr(pipeline, "set_adapters"):
            pipeline.set_adapters(["tentgent"], adapter_weights=[self._adapter.lora_scale])

    def _require_pipeline(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Diffusers image generation pipeline is not loaded yet; "
                "call generate_image() first."
            )
        return self._pipeline


def _adapter_weight_path(adapter: ImageGenerationAdapterSelection) -> os.PathLike[str]:
    source_path = adapter.source_path
    if adapter.weight_file:
        return source_path / adapter.weight_file
    if source_path.is_file():
        return source_path
    candidates = sorted(source_path.rglob("*.safetensors"))
    if len(candidates) == 1:
        return candidates[0]
    if not candidates:
        raise FileNotFoundError(
            f"image LoRA adapter `{adapter.adapter_ref[:12]}` has no .safetensors weights"
        )
    names = ", ".join(str(path.relative_to(source_path)) for path in candidates)
    raise ValueError(
        f"image LoRA adapter `{adapter.adapter_ref[:12]}` has multiple .safetensors "
        f"weights; select one in adapter metadata. Candidates: {names}"
    )


def _load_diffusers_deps() -> _DiffusersDeps:
    try:
        import torch
        from diffusers import (
            AutoPipelineForImage2Image,
            AutoPipelineForInpainting,
            ControlNetModel,
            DiffusionPipeline,
            StableDiffusionControlNetPipeline,
        )
        from PIL import Image as PILImage
    except ModuleNotFoundError as exc:
        if exc.name in {"torch", "diffusers", "PIL"}:
            raise missing_backend_dependency(exc.name) from exc
        raise
    except ImportError as exc:
        raise RuntimeError(
            "Diffusers image generation requires a recent runtime with "
            "text-to-image, image-to-image, inpainting, and ControlNet pipeline support."
        ) from exc

    return _DiffusersDeps(
        torch=torch,
        DiffusionPipeline=DiffusionPipeline,
        AutoPipelineForImage2Image=AutoPipelineForImage2Image,
        AutoPipelineForInpainting=AutoPipelineForInpainting,
        ControlNetModel=ControlNetModel,
        StableDiffusionControlNetPipeline=StableDiffusionControlNetPipeline,
        PILImage=PILImage,
    )


def _load_transform_image(
    pil_image: Any,
    image_path: os.PathLike[str],
    width: int,
    height: int,
) -> Any:
    with pil_image.open(image_path) as image:
        image = image.convert("RGB")
        if image.size != (width, height):
            image = image.resize((width, height))
        return image


def _pipeline_class_for_workflow(
    deps: _DiffusersDeps,
    workflow: ImageGenerationWorkflowKind,
) -> Any:
    if workflow == ImageGenerationWorkflowKind.CONTROL:
        return deps.StableDiffusionControlNetPipeline
    if workflow == ImageGenerationWorkflowKind.INPAINT:
        return deps.AutoPipelineForInpainting
    if workflow == ImageGenerationWorkflowKind.IMAGE_TO_IMAGE:
        return deps.AutoPipelineForImage2Image
    return deps.DiffusionPipeline


def _pipeline_key_for_request(
    workflow: ImageGenerationWorkflowKind,
    request: ImageGenerationRequest,
) -> str:
    if workflow == ImageGenerationWorkflowKind.CONTROL and request.control is not None:
        return f"control:{request.control.control_ref}:{request.control.control_kind}"
    return workflow.value
