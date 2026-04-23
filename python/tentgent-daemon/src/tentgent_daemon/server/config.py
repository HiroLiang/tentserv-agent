from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ServerConfig:
    server_ref: str
    model_ref: str
    host: str
    port: int
    home: Path | None
    lazy_load: bool
    idle_seconds: int | None

