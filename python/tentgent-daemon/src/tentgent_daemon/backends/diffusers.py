from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Any

from .base import ImageGenerationBackend
from ..runtime.image_generation import (
    ImageGenerationAdapterSelection,
    ImageGenerationRequest,
    ImageGenerationResult,
    load_normalized_control_image,
    load_normalized_inpaint_images,
    write_image_generation_output,
)
from ..runtime.profile_deps import missing_profile_dependency
from ..runtime.records import StoredModelRecord


@dataclass(frozen=True)
class DiffusersDeps:
    torch: Any
    DiffusionPipeline: Any
    AutoPipelineForImage2Image: Any
    AutoPipelineForInpainting: Any
    ControlNetModel: Any
    StableDiffusionControlNetPipeline: Any
    PILImage: Any


class DiffusersImageGenerationBackend(ImageGenerationBackend):
    def __init__(self) -> None:
        self._deps = _load_diffusers_deps()
        self._record: StoredModelRecord | None = None
        self._pipeline: Any | None = None
        self._pipeline_workflow: str | None = None
        self._device = _detect_device(self._deps.torch)
        self._adapter: ImageGenerationAdapterSelection | None = None

    def load(self, record: StoredModelRecord) -> None:
        self._record = record
        self._pipeline = None
        self._pipeline_workflow = None

    def _load_pipeline(self, workflow: str, request: ImageGenerationRequest) -> Any:
        if self._record is None:
            raise RuntimeError(
                "Diffusers image generation backend is not loaded yet; "
                "call load() before generate_image()."
            )
        pipeline_key = _pipeline_key_for_request(workflow, request)
        if self._pipeline is not None and self._pipeline_workflow == pipeline_key:
            return self._pipeline

        load_kwargs = _diffusers_load_kwargs(self._record, self._deps.torch, self._device)
        pipeline_class = _pipeline_class_for_workflow(self._deps, workflow)
        pipeline_kwargs: dict[str, object] = dict(load_kwargs)
        if workflow == "control":
            if request.control is None:
                raise ValueError("Diffusers ControlNet generation requires a control adapter")
            controlnet = self._deps.ControlNetModel.from_pretrained(
                str(request.control.source_path),
                local_files_only=True,
                **_controlnet_load_kwargs(self._deps.torch, self._device),
            )
            pipeline_kwargs["controlnet"] = controlnet
        pipeline = pipeline_class.from_pretrained(
            str(self._record.variant_source_path),
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

    def select_adapter(self, adapter: ImageGenerationAdapterSelection | None) -> None:
        self._adapter = adapter
        if self._pipeline is not None:
            self._apply_selected_adapter()

    def generate_image(self, request: ImageGenerationRequest) -> ImageGenerationResult:
        workflow = _workflow_for_request(request)
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
        if request.control_image_path is not None:
            if request.control is None:
                raise ValueError("image control generation requires a control adapter")
            if request.control_strength is None:
                raise ValueError("image control strength is required")
            kwargs["image"] = load_normalized_control_image(
                request.control_image_path,
                request.width,
                request.height,
            )
            kwargs["controlnet_conditioning_scale"] = request.control_strength
        elif request.input_image_path is not None:
            if request.strength is None:
                raise ValueError("image denoising strength is required")
            if request.mask_image_path is not None:
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

    def _apply_selected_adapter(self) -> None:
        if self._adapter is None:
            return
        pipeline = self._require_loaded()
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

    def release(self) -> None:
        self._record = None
        self._pipeline = None
        self._pipeline_workflow = None
        self._adapter = None

        torch = self._deps.torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        if torch.backends.mps.is_available():
            torch.mps.empty_cache()

    def _require_loaded(self) -> Any:
        if self._record is None or self._pipeline is None:
            raise RuntimeError(
                "Diffusers image generation backend is not loaded yet; "
                "call load() before generate_image()."
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


def _load_diffusers_deps() -> DiffusersDeps:
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
            raise missing_profile_dependency("local-model", exc.name) from exc
        raise
    except ImportError as exc:
        raise RuntimeError(
            "Diffusers image generation requires a recent local-model runtime "
            "with text-to-image, image-to-image, inpainting, and ControlNet "
            "pipeline support. Run `tentgent runtime bootstrap --profile local-model`."
        ) from exc

    return DiffusersDeps(
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


def _workflow_for_request(request: ImageGenerationRequest) -> str:
    if request.control_image_path is not None:
        return "control"
    if request.mask_image_path is not None:
        return "inpaint"
    if request.input_image_path is not None:
        return "image-to-image"
    return "text-to-image"


def _pipeline_class_for_workflow(deps: DiffusersDeps, workflow: str) -> Any:
    if workflow == "control":
        return deps.StableDiffusionControlNetPipeline
    if workflow == "inpaint":
        return deps.AutoPipelineForInpainting
    if workflow == "image-to-image":
        return deps.AutoPipelineForImage2Image
    return deps.DiffusionPipeline


def _pipeline_key_for_request(workflow: str, request: ImageGenerationRequest) -> str:
    if workflow == "control" and request.control is not None:
        return f"control:{request.control.control_ref}:{request.control.control_kind}"
    return workflow


def _diffusers_load_kwargs(
    record: StoredModelRecord, torch: Any, device: Any
) -> dict[str, object]:
    kwargs: dict[str, object] = {
        "torch_dtype": _torch_dtype_for_device(torch, device),
    }
    if _declares_missing_safety_checker(record):
        kwargs["safety_checker"] = None
    return kwargs


def _controlnet_load_kwargs(torch: Any, device: Any) -> dict[str, object]:
    return {
        "torch_dtype": _torch_dtype_for_device(torch, device),
    }


def _declares_missing_safety_checker(record: StoredModelRecord) -> bool:
    model_index_path = record.variant_source_path / "model_index.json"
    try:
        raw = json.loads(model_index_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False

    return "safety_checker" in raw and not (
        record.variant_source_path / "safety_checker"
    ).exists()


def _detect_device(torch: Any) -> Any:
    requested = os.environ.get("TENTGENT_IMAGE_GENERATION_DEVICE", "").strip().lower()
    if requested:
        return _requested_device(torch, requested)

    if torch.cuda.is_available():
        return torch.device("cuda")
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _requested_device(torch: Any, requested: str) -> Any:
    if requested == "cpu":
        return torch.device("cpu")
    if requested == "cuda":
        if torch.cuda.is_available():
            return torch.device("cuda")
        raise RuntimeError(
            "TENTGENT_IMAGE_GENERATION_DEVICE=cuda was requested, but CUDA is not available"
        )
    if requested == "mps":
        if torch.backends.mps.is_available():
            return torch.device("mps")
        raise RuntimeError(
            "TENTGENT_IMAGE_GENERATION_DEVICE=mps was requested, but PyTorch MPS is not available"
        )
    raise RuntimeError(
        "unsupported TENTGENT_IMAGE_GENERATION_DEVICE value "
        f"`{requested}`; expected one of: cpu, mps, cuda"
    )


def _torch_dtype_for_device(torch: Any, device: Any) -> Any:
    requested = os.environ.get("TENTGENT_IMAGE_GENERATION_TORCH_DTYPE", "").strip().lower()
    if requested:
        return _requested_torch_dtype(torch, requested)

    device_type = getattr(device, "type", str(device))
    if device_type == "cuda":
        return torch.float16
    return torch.float32


def _requested_torch_dtype(torch: Any, requested: str) -> Any:
    if requested in {"float32", "fp32"}:
        return torch.float32
    if requested in {"float16", "fp16"}:
        return torch.float16
    raise RuntimeError(
        "unsupported TENTGENT_IMAGE_GENERATION_TORCH_DTYPE value "
        f"`{requested}`; expected one of: float32, float16"
    )
