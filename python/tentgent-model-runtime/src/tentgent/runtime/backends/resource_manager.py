from __future__ import annotations

import math
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from threading import RLock
from time import monotonic
from typing import Any, Generic, TypeVar

from .base import BackendModel
from .records import ModelRecord

ModelT = TypeVar("ModelT", bound=BackendModel)


class ModelResourceInvalidatedError(RuntimeError):
    """A failed resource cannot be reused by an already reserved lease."""


class ModelResourceCleanupError(RuntimeError):
    """The resource is quarantined until runtime shutdown can retry cleanup."""


@dataclass(frozen=True, slots=True)
class ModelResourceKey:
    kind: str
    model_ref: str
    source_path: str


@dataclass(slots=True)
class _LoadedModelResource(Generic[ModelT]):
    model: ModelT
    lock: RLock
    record: ModelRecord
    idle_timeout_seconds: float
    last_used_at: float
    active_leases: int = 0
    load_error: str | None = None
    cleanup_error: str | None = None


class ResourceManager(Generic[ModelT]):
    def __init__(
        self,
        *,
        model_factory: Callable[[Any], ModelT],
        model_idle_seconds: float = 0.0,
        clock: Callable[[], float] = monotonic,
    ) -> None:
        _validate_idle_timeout(
            model_idle_seconds,
            name="model_idle_seconds",
        )
        self._lock = RLock()
        self._model_resources: dict[
            ModelResourceKey,
            _LoadedModelResource[ModelT],
        ] = {}
        self._default_model_idle_seconds = model_idle_seconds
        self._model_factory = model_factory
        self._clock = clock

    @contextmanager
    def lease_model(
        self,
        kind: Any,
        record: ModelRecord,
        *,
        idle_timeout_seconds: float | None = None,
    ) -> Iterator[ModelT]:
        key = ModelResourceKey(
            kind=_kind_value(kind),
            model_ref=record.model_ref,
            source_path=str(record.source_path),
        )
        resource = self._reserve_model_resource(
            key,
            kind,
            record,
            idle_timeout_seconds=idle_timeout_seconds,
        )

        resource.lock.acquire()
        try:
            self._check_resource(resource)
            try:
                if not resource.model.is_loaded:
                    resource.model.load(record)
                if not resource.model.is_loaded:
                    raise RuntimeError("backend load completed without a loaded model")
            except BaseException as error:
                # Only load-stage failures invalidate a model. Exceptions from
                # the caller's inference body must not discard a healthy model.
                with self._lock:
                    resource.load_error = str(error)
                self._release_resource(resource)
                raise
            yield resource.model
        finally:
            resource.last_used_at = self._clock()
            resource.lock.release()
            with self._lock:
                resource.active_leases -= 1
                self._remove_failed_resource(key, resource)
            if resource.idle_timeout_seconds == 0 and resource.load_error is None:
                self.release_idle()

    def release_idle(self) -> int:
        now = self._clock()
        resources_to_release: list[
            tuple[ModelResourceKey, _LoadedModelResource[ModelT]]
        ] = []

        with self._lock:
            for key, resource in list(self._model_resources.items()):
                timeout = resource.idle_timeout_seconds
                if resource.active_leases > 0 or resource.cleanup_error is not None:
                    continue
                if now - resource.last_used_at < timeout:
                    continue
                if not resource.lock.acquire(blocking=False):
                    continue
                # Reserve while releasing outside the manager lock. New leases
                # wait on this object instead of loading a second copy of it.
                resource.active_leases += 1
                resources_to_release.append((key, resource))

        first_error: ModelResourceCleanupError | None = None
        for key, resource in resources_to_release:
            try:
                self._release_resource(resource)
            except ModelResourceCleanupError as error:
                first_error = first_error or error
            finally:
                with self._lock:
                    resource.active_leases -= 1
                    if resource.active_leases == 0 and resource.cleanup_error is None:
                        self._model_resources.pop(key, None)
                resource.lock.release()

        if first_error is not None:
            raise first_error
        return len(resources_to_release)

    def release_all(self) -> None:
        with self._lock:
            resources = list(self._model_resources.items())

        first_error: ModelResourceCleanupError | None = None
        for key, resource in resources:
            try:
                with resource.lock:
                    self._release_resource(resource)
            except ModelResourceCleanupError as error:
                first_error = first_error or error
            else:
                with self._lock:
                    if self._model_resources.get(key) is resource:
                        self._model_resources.pop(key)
        if first_error is not None:
            raise first_error

    def snapshot(self) -> dict[str, Any]:
        with self._lock:
            model_resources = [
                {
                    "kind": key.kind,
                    "model_ref": key.model_ref,
                    "source_path": key.source_path,
                    "loaded": resource.model.is_loaded,
                    "active_leases": resource.active_leases,
                    "idle_timeout_seconds": resource.idle_timeout_seconds,
                    "idle_age_seconds": round(self._clock() - resource.last_used_at, 3),
                    "state": (
                        "quarantined"
                        if resource.cleanup_error is not None
                        else "invalidated"
                        if resource.load_error is not None
                        else "available"
                    ),
                    "load_error": resource.load_error,
                    "cleanup_error": resource.cleanup_error,
                }
                for key, resource in self._model_resources.items()
            ]

        return {
            "default_model_idle_seconds": (self._default_model_idle_seconds),
            "model_resource_count": len(model_resources),
            "model_resources": model_resources,
        }

    def _reserve_model_resource(
        self,
        key: ModelResourceKey,
        kind: Any,
        record: ModelRecord,
        *,
        idle_timeout_seconds: float | None,
    ) -> _LoadedModelResource[ModelT]:
        if idle_timeout_seconds is not None:
            _validate_idle_timeout(
                idle_timeout_seconds,
                name="idle_timeout_seconds",
            )
        with self._lock:
            resource = self._model_resources.get(key)
            if resource is not None:
                self._check_resource(resource)
                if idle_timeout_seconds is not None:
                    resource.idle_timeout_seconds = idle_timeout_seconds
                resource.active_leases += 1
                return resource

            resource = _LoadedModelResource(
                model=self._model_factory(kind),
                lock=RLock(),
                record=record,
                idle_timeout_seconds=(
                    self._default_model_idle_seconds
                    if idle_timeout_seconds is None
                    else idle_timeout_seconds
                ),
                last_used_at=self._clock(),
            )
            self._model_resources[key] = resource
            resource.active_leases += 1
            return resource

    def _release_resource(self, resource: _LoadedModelResource[ModelT]) -> None:
        # The caller holds resource.lock. Keep only diagnostics, never exception
        # tracebacks (which may retain partially allocated tensors), in the slot.
        try:
            resource.model.release()
            if resource.model.is_loaded:
                raise RuntimeError(
                    "backend release completed but the model is still loaded"
                )
        except BaseException as error:
            with self._lock:
                resource.cleanup_error = str(error)
            raise ModelResourceCleanupError(
                f"model cleanup failed; resource quarantined until runtime shutdown: {error}"
            ) from error

    @staticmethod
    def _check_resource(resource: _LoadedModelResource[ModelT]) -> None:
        if resource.cleanup_error is not None:
            raise ModelResourceCleanupError(
                f"model resource is quarantined until runtime shutdown: {resource.cleanup_error}"
            )
        if resource.load_error is not None:
            raise ModelResourceInvalidatedError(
                f"model resource was invalidated by failed loading: {resource.load_error}"
            )

    def _remove_failed_resource(
        self, key: ModelResourceKey, resource: _LoadedModelResource[ModelT]
    ) -> None:
        # Called under the manager lock only after every reserved waiter exits.
        if (
            resource.active_leases == 0
            and resource.load_error is not None
            and resource.cleanup_error is None
            and self._model_resources.get(key) is resource
        ):
            self._model_resources.pop(key)


def _validate_idle_timeout(value: float, *, name: str) -> None:
    if not math.isfinite(value) or value < 0:
        raise ValueError(f"{name} must be finite and non-negative")


def _kind_value(kind: Any) -> str:
    value = getattr(kind, "value", None)
    if isinstance(value, str):
        return value
    return str(kind)
