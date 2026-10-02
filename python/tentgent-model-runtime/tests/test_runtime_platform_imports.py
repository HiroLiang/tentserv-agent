"""Base runtime imports and optional memory telemetry on non-POSIX platforms."""

import os
import subprocess
import sys
import textwrap
from types import SimpleNamespace
from unittest.mock import Mock

import pytest
from tentgent.runtime.backends.transformers import lora_tuning


def test_base_runtime_imports_when_resource_module_is_unavailable():
    # A fresh interpreter covers the entire package import chain rather than
    # hiding an eagerly imported resource module in this test process's cache.
    probe = textwrap.dedent(
        """
        import importlib.abc
        import sys

        class MissingResource(importlib.abc.MetaPathFinder):
            def find_spec(self, fullname, path=None, target=None):
                if fullname == "resource":
                    raise ModuleNotFoundError("resource unavailable", name="resource")
                return None

        sys.modules.pop("resource", None)
        sys.meta_path.insert(0, MissingResource())
        from tentgent.runtime.server.app import create_app
        from tentgent.runtime.backends.transformers.lora_tuning import process_peak_memory_gb
        assert callable(create_app)
        assert process_peak_memory_gb() is None
        print("runtime import without resource passed")
        """
    )
    result = subprocess.run(
        [sys.executable, "-c", probe],
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        capture_output=True,
        text=True,
        timeout=20,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "runtime import without resource passed" in result.stdout


def test_unknown_cpu_memory_does_not_become_zero(monkeypatch):
    monkeypatch.setattr(lora_tuning, "resource", None)
    assert lora_tuning.process_peak_memory_gb() is None
    assert lora_tuning.memory_gb(Mock(), SimpleNamespace(type="cpu")) is None


@pytest.mark.parametrize("platform,multiplier", [("darwin", 1), ("linux", 1024)])
def test_posix_peak_memory_preserves_native_units(monkeypatch, platform, multiplier):
    fake_resource = SimpleNamespace(
        RUSAGE_SELF=0,
        getrusage=Mock(return_value=SimpleNamespace(ru_maxrss=123_456)),
    )
    monkeypatch.setattr(lora_tuning, "resource", fake_resource)
    monkeypatch.setattr(lora_tuning.sys, "platform", platform)
    assert lora_tuning.process_peak_memory_gb() == 123_456 * multiplier / 1_000_000_000
    fake_resource.getrusage.assert_called_once_with(0)


@pytest.mark.parametrize("device_type", ["cuda", "mps"])
def test_device_memory_does_not_depend_on_resource(monkeypatch, device_type):
    monkeypatch.setattr(lora_tuning, "resource", None)
    torch = Mock()
    torch.cuda.max_memory_allocated.return_value = 2_000_000_000
    torch.mps.current_allocated_memory.return_value = 3_000_000_000
    expected = 2.0 if device_type == "cuda" else 3.0
    assert lora_tuning.memory_gb(torch, SimpleNamespace(type=device_type)) == expected


@pytest.mark.parametrize("measured", [None, 0.0, 1.25])
def test_training_event_distinguishes_unavailable_and_measured_zero(measured):
    events = []
    lora_tuning.emit_train(
        1,
        2,
        0.5,
        SimpleNamespace(param_groups=[{"lr": 0.01}]),
        1.0,
        3,
        3,
        measured,
        emit=events.append,
    )
    if measured is None:
        assert "peak_memory_gb" not in events[0]
    else:
        assert events[0]["peak_memory_gb"] == measured
    assert events[0]["trained_tokens"] == 3
