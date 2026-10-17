from pathlib import Path
from unittest.mock import Mock, patch

import pytest

from tentgent.runtime.backends.llama_cpp.chat import LlamaCppChatModel
from tentgent.runtime.backends.llama_cpp.common import resolve_gguf_path
from tentgent.runtime.backends.llama_cpp.embedding import LlamaCppEmbeddingModel
from tentgent.runtime.backends.records import ModelFormat, ModelRecord


PROJECTORS = ("mmproj-model-f16-4B.gguf", "MMPROJ_F32.GGUF", "mmproj.gguf")


def write_files(root: Path, names: tuple[str, ...]) -> None:
    for name in names:
        (root / name).write_bytes(b"fixture")


@pytest.mark.parametrize("name", ["gemma-3-4b-it-q4_0.gguf", "model.GGUF"])
def test_selects_primary_model_with_projector_sidecars(tmp_path, name):
    write_files(tmp_path, (name, *PROJECTORS))
    assert resolve_gguf_path(tmp_path) == tmp_path / name
    assert resolve_gguf_path(tmp_path / name) == tmp_path / name
    assert all((tmp_path / projector).is_file() for projector in PROJECTORS)


@pytest.mark.parametrize("name", PROJECTORS)
def test_projector_alone_is_not_a_model(tmp_path, name):
    write_files(tmp_path, (name,))
    with pytest.raises(FileNotFoundError, match="no primary GGUF model"):
        resolve_gguf_path(tmp_path)
    with pytest.raises(ValueError, match="got projector"):
        resolve_gguf_path(tmp_path / name)


def test_multiple_primary_models_remain_ambiguous(tmp_path):
    write_files(tmp_path, ("model-q4.gguf", "model-q8.GGUF", *PROJECTORS))
    with pytest.raises(ValueError, match="multiple primary GGUF") as error:
        resolve_gguf_path(tmp_path)
    assert "model-q4.gguf, model-q8.GGUF" in str(error.value)
    assert "mmproj" not in str(error.value)


def test_gguf_directories_and_nested_models_are_not_candidates(tmp_path):
    nested = tmp_path / "nested.gguf"
    nested.mkdir()
    write_files(nested, ("nested-model.gguf",))
    with pytest.raises(FileNotFoundError, match="no primary GGUF model"):
        resolve_gguf_path(tmp_path)
    write_files(tmp_path, ("model.gguf",))
    assert resolve_gguf_path(tmp_path) == tmp_path / "model.gguf"


def test_projector_prefix_requires_a_separator(tmp_path):
    write_files(tmp_path, ("mmprojection-model.gguf",))
    assert resolve_gguf_path(tmp_path) == tmp_path / "mmprojection-model.gguf"


def test_non_gguf_file_is_rejected(tmp_path):
    write_files(tmp_path, ("README.md",))
    with pytest.raises(ValueError, match="expected a GGUF file"):
        resolve_gguf_path(tmp_path / "README.md")


@pytest.mark.parametrize(
    "backend_class,module,options",
    [
        (LlamaCppChatModel, "chat", {"n_ctx": 2048, "verbose": False}),
        (LlamaCppEmbeddingModel, "embedding", {"embedding": True, "verbose": False}),
    ],
)
def test_backends_load_primary_model_with_projector_present(
    tmp_path, backend_class, module, options
):
    name = "gemma-3-4b-it-q4_0.gguf"
    write_files(tmp_path, (name, *PROJECTORS))
    record = ModelRecord(
        model_ref="gemma-fixture", source_path=tmp_path, primary_format=ModelFormat.GGUF
    )
    llama = Mock()
    backend = backend_class()
    with patch(
        f"tentgent.runtime.backends.llama_cpp.{module}.load_llama_class",
        return_value=llama,
    ):
        backend.load(record)
    llama.assert_called_once_with(model_path=str(tmp_path / name), **options)
    assert backend.is_loaded
    backend.release()
    assert not backend.is_loaded
