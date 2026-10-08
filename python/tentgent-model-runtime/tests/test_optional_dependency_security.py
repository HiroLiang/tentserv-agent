"""Behavioral regressions for patched optional dependencies, with no network I/O.

Upstream advisories:
https://github.com/fsspec/filesystem_spec/security/advisories/GHSA-27vj-qcqg-25rc
https://github.com/aio-libs/multidict/security/advisories/GHSA-54p9-h82j-f925
"""

from __future__ import annotations

import gc
import sys

import pytest


@pytest.fixture
def reference_dependencies():
    reference = pytest.importorskip("fsspec.implementations.reference")
    exceptions = pytest.importorskip("jinja2.exceptions")
    return reference.ReferenceFileSystem, exceptions.SecurityError


def reference_document(sink, expression):
    document = {"version": 1, "refs": {}, "templates": {"unused": "value"}}
    if sink == "refs":
        document["refs"] = {"data": [expression]}
    elif sink == "templates":
        document["templates"] = {"target": expression}
        document["refs"] = {"data": ["{{ target() }}"]}
    else:
        generator = {
            "key": "data",
            "url": "memory://dependency-security-fixture",
            "offset": "0",
            "length": "3",
            "dimensions": {"i": [0]},
        }
        generator[sink.removeprefix("gen-")] = expression
        document["gen"] = [generator]
    return document


@pytest.mark.parametrize(
    "sink", ["refs", "templates", "gen-key", "gen-url", "gen-offset", "gen-length"]
)
def test_reference_templates_reject_unsafe_attributes(reference_dependencies, sink):
    reference_fs, security_error = reference_dependencies
    # Only inspect a literal's type name: no function, shell, file or network
    # payload is executed, even when proving that an older version fails.
    document = reference_document(sink, "{{ ''.__class__.__name__ }}")
    with pytest.raises(security_error, match="__class__"):
        reference_fs(document, simple_templates=False, remote_protocol="memory")


def test_reference_default_mode_does_not_evaluate_generators(reference_dependencies):
    reference_fs, _ = reference_dependencies
    document = reference_document("gen-key", "{{ ''.__class__.__name__ }}")
    filesystem = reference_fs(document, remote_protocol="memory")
    assert filesystem.references == {}


@pytest.mark.parametrize("sink", ["refs", "templates", "gen-url"])
def test_reference_normal_templates_still_read_bytes(reference_dependencies, sink):
    reference_fs, _ = reference_dependencies
    memory_fs = pytest.importorskip("fsspec.implementations.memory").MemoryFileSystem()
    path = f"/dependency-security-fixture-{sink}"
    memory_fs.pipe_file(path, b"abcdef")
    try:
        if sink == "templates":
            document = reference_document(sink, f"memory://{path}/{{{{ suffix }}}}")
            document["refs"] = {"data": ["{{ target(suffix='') }}", 0, 3]}
        else:
            document = reference_document(sink, "{{ target }}")
            document["templates"]["target"] = f"memory://{path}"
            if sink == "refs":
                document["refs"]["data"] += [0, 3]
        filesystem = reference_fs(document, simple_templates=False, fs=memory_fs)
        assert filesystem.cat_file("data") == b"abc"
    finally:
        memory_fs.rm(path)


@pytest.mark.parametrize("class_name", ["MultiDict", "CIMultiDict"])
@pytest.mark.parametrize("operation", ["reflected-union", "subtraction"])
def test_multidict_items_operations_release_operand_references(class_name, operation):
    if sys.implementation.name != "cpython":
        pytest.skip("CPython reference counts and multidict C extension are required")
    extension = pytest.importorskip(
        "multidict._multidict", reason="optional multidict C extension is not installed"
    )
    mapping = getattr(extension, class_name)({"seed": "existing"})
    items = mapping.items()
    sentinel = object()
    operand = [(f"field-{index}", sentinel) for index in range(8)]
    gc.collect()
    before = sys.getrefcount(sentinel)
    # 128 operand visits detect the C strong-reference leak without a memory
    # stress test. Results must be discarded before comparing reference counts.
    for _ in range(16):
        if operation == "reflected-union":
            result = operand | items
            assert len(result) == 9
        else:
            result = items - operand
            assert result == {("seed", "existing")}
        del result
    gc.collect()
    assert sys.getrefcount(sentinel) == before
