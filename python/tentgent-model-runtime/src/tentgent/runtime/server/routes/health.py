from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Request


router = APIRouter()


@router.get("/healthz")
def healthz(request: Request) -> dict[str, Any]:
    request.app.state.task_manager.touch_activity()
    return request.app.state.lifecycle.snapshot()
