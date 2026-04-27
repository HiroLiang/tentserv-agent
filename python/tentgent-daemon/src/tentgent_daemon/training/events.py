"""Small event helpers shared by training runners."""

from __future__ import annotations

import json
from collections.abc import Mapping
from typing import Any


def emit(event: Mapping[str, Any]) -> None:
    """Write one Tentgent event to stdout."""
    print(json.dumps(dict(event), sort_keys=True), flush=True)
