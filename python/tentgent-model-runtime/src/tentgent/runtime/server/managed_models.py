from __future__ import annotations

import os
import tomllib
from pathlib import Path
from typing import Any

from fastapi import HTTPException, Request

from tentgent.runtime.backends.chat import ChatModelKind
from tentgent.runtime.backends.embedding import EmbeddingModelKind
from tentgent.runtime.backends.records import ModelCapability, ModelFormat, ModelRecord
from tentgent.runtime.backends.rerank import RerankModelKind

from .lifecycle import RuntimeCapability, RuntimeServerConfig
from .routes.payloads import ModelRecordPayload, model_record


def resolve_request_model(
    payload_model: ModelRecordPayload | None,
    request: Request,
    *,
    required_capability: ModelCapability,
) -> ModelRecord:
    if payload_model is not None:
        record = model_record(payload_model)
    else:
        config: RuntimeServerConfig = request.app.state.runtime_config
        if config.model_ref is None:
            raise HTTPException(
                status_code=400,
                detail="`model` is required when the runtime was not started with `--model-ref`",
            )
        record = load_managed_model_record(config.model_ref, home=config.home)

    if record.capabilities and required_capability not in record.capabilities:
        raise HTTPException(
            status_code=400,
            detail=(
                f"model `{record.model_ref}` does not advertise "
                f"`{required_capability.value}` capability"
            ),
        )
    return record


def load_managed_model_record(model_ref: str, *, home: Path | None) -> ModelRecord:
    model_dir = _resolve_model_dir(model_ref, home=home)
    metadata = _read_toml(model_dir / "model.toml")
    primary_format = ModelFormat(str(metadata["primary_format"]))
    variant = _read_toml(model_dir / "variants" / primary_format.value / "variant.toml")
    relative_source_path = str(variant.get("relative_source_path", "source"))

    capabilities = frozenset(
        ModelCapability(str(value))
        for value in metadata.get("model_capabilities", ["chat"])
    )

    return ModelRecord(
        model_ref=str(metadata["model_ref"]),
        short_ref=str(metadata.get("short_ref") or str(metadata["model_ref"])[:12]),
        source_path=model_dir / "variants" / primary_format.value / relative_source_path,
        primary_format=primary_format,
        capabilities=capabilities,
        source_repo=_optional_str(metadata.get("source_repo")),
        source_revision=_optional_str(metadata.get("source_revision")),
    )


def infer_chat_model_kind(record: ModelRecord) -> ChatModelKind:
    if record.primary_format == ModelFormat.MLX:
        return ChatModelKind.MLX
    if record.primary_format == ModelFormat.GGUF:
        return ChatModelKind.LLAMA_CPP
    if record.primary_format == ModelFormat.SAFETENSORS:
        return ChatModelKind.TRANSFORMERS
    raise HTTPException(
        status_code=400,
        detail=(
            f"chat server does not support `{record.primary_format.value}` "
            "model format"
        ),
    )


def infer_embedding_model_kind(record: ModelRecord) -> EmbeddingModelKind:
    if record.primary_format == ModelFormat.GGUF:
        return EmbeddingModelKind.LLAMA_CPP
    if record.primary_format == ModelFormat.MLX:
        return EmbeddingModelKind.MLX
    if record.primary_format == ModelFormat.SAFETENSORS:
        return EmbeddingModelKind.TRANSFORMERS
    raise HTTPException(
        status_code=400,
        detail=(
            f"embedding server does not support `{record.primary_format.value}` "
            "model format"
        ),
    )


def infer_rerank_model_kind(record: ModelRecord) -> RerankModelKind:
    if record.primary_format == ModelFormat.MLX:
        return RerankModelKind.MLX
    if record.primary_format == ModelFormat.SAFETENSORS:
        return RerankModelKind.TRANSFORMERS
    raise HTTPException(
        status_code=400,
        detail=(
            f"rerank server does not support `{record.primary_format.value}` "
            "model format"
        ),
    )


def infer_model_kind_for_capability(
    capability: RuntimeCapability,
    record: ModelRecord,
) -> str:
    if capability == RuntimeCapability.CHAT:
        return infer_chat_model_kind(record).value
    if capability == RuntimeCapability.EMBEDDING:
        return infer_embedding_model_kind(record).value
    if capability == RuntimeCapability.RERANK:
        return infer_rerank_model_kind(record).value
    raise ValueError(f"capability `{capability.value}` has no server model-kind inference")


def runtime_home(home: Path | None) -> Path:
    if home is not None:
        return home.expanduser().resolve()
    raw_home = os.environ.get("TENTGENT_HOME")
    if raw_home:
        return Path(raw_home).expanduser().resolve()
    raise HTTPException(
        status_code=400,
        detail=(
            "`TENTGENT_HOME` or `--home` is required to resolve a managed "
            "server model"
        ),
    )


def _resolve_model_dir(model_ref: str, *, home: Path | None) -> Path:
    store_dir = runtime_home(home) / "models" / "store"
    exact = store_dir / model_ref
    if exact.is_dir():
        return exact

    matches = sorted(path for path in store_dir.glob(f"{model_ref}*") if path.is_dir())
    if not matches:
        raise FileNotFoundError(f"managed model `{model_ref}` was not found")
    if len(matches) > 1:
        short_refs = ", ".join(path.name[:12] for path in matches)
        raise ValueError(f"managed model ref `{model_ref}` is ambiguous: {short_refs}")
    return matches[0]


def _read_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def _optional_str(value: object) -> str | None:
    if value is None:
        return None
    return str(value)
