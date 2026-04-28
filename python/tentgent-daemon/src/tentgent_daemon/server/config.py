from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ServerConfig:
    server_ref: str
    runtime_kind: str
    model_ref: str | None
    provider: str | None
    provider_model: str | None
    host: str
    port: int
    home: Path | None
    lazy_load: bool
    idle_seconds: int | None

    @property
    def is_cloud(self) -> bool:
        return self.runtime_kind == "cloud"

    @property
    def runtime_label(self) -> str:
        if self.is_cloud:
            return f"{self.provider}:{self.provider_model}"
        return self.model_ref or "(missing)"
