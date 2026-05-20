from __future__ import annotations

import platform
import sys


def ensure_backend_supported(backend: str) -> None:
    if backend not in {"mlx", "mlx_audio", "mlx_diffusion", "mlx_vlm"}:
        return

    if _is_apple_silicon_macos():
        return

    raise RuntimeError(
        f"backend `{backend}` is supported only on Apple Silicon macOS; "
        f"current platform is {sys.platform}-{platform.machine()}"
    )


def _is_apple_silicon_macos() -> bool:
    machine = platform.machine().lower()
    return sys.platform == "darwin" and machine in {"arm64", "aarch64"}
