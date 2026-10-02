"""Reject stale or editable Python runtime code in the native source gate."""

import sysconfig
from pathlib import Path


def verify_source_tree(source: Path, installed: Path, site_packages: Path) -> int:
    source, installed = source.resolve(), installed.resolve()
    if installed == source or not installed.is_relative_to(site_packages.resolve()):
        raise RuntimeError(
            "release Python runtime must come from installed site-packages"
        )

    source_files = {path.relative_to(source) for path in source.rglob("*.py")}
    installed_files = {path.relative_to(installed) for path in installed.rglob("*.py")}
    if not source_files:
        raise RuntimeError("release Python source tree is empty")
    if source_files != installed_files:
        missing = sorted(str(path) for path in source_files - installed_files)
        extra = sorted(str(path) for path in installed_files - source_files)
        raise RuntimeError(
            f"installed Python source files differ: missing={missing}, extra={extra}"
        )
    changed = sorted(
        str(path)
        for path in source_files
        if (source / path).read_bytes() != (installed / path).read_bytes()
    )
    if changed:
        raise RuntimeError(f"installed Python source content differs: {changed}")
    return len(source_files)


def main() -> None:
    import tentgent.runtime

    source = (
        Path(__file__).resolve().parent.parent
        / "python/tentgent-model-runtime/src/tentgent/runtime"
    )
    installed = Path(tentgent.runtime.__file__).resolve().parent
    count = verify_source_tree(source, installed, Path(sysconfig.get_path("purelib")))
    print(
        f"Installed Python runtime matches this checkout: {count} source files ({installed})"
    )


if __name__ == "__main__":
    main()
