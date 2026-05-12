from __future__ import annotations


PROFILE_BOOTSTRAP_HINTS = {
    "local-model": "tentgent runtime bootstrap --profile local-model",
    "training": "tentgent runtime bootstrap --profile training",
    "full": "tentgent runtime bootstrap --profile full",
}


def missing_profile_dependency(profile: str, package: str | None) -> RuntimeError:
    package_label = f" Missing Python package: {package}." if package else ""
    command = PROFILE_BOOTSTRAP_HINTS.get(
        profile,
        f"tentgent runtime bootstrap --profile {profile}",
    )
    return RuntimeError(
        f"{profile} dependencies are not installed; run `{command}`."
        f"{package_label}"
    )
