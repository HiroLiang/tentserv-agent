from __future__ import annotations


def missing_backend_dependency(package: str) -> RuntimeError:
    return RuntimeError(
        "Python model runtime dependency is not installed. "
        f"Missing Python package: {package}."
    )
