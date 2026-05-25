from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Request


router = APIRouter(prefix="/v1/lifecycle")


@router.post("/shutdown")
def shutdown(request: Request) -> dict[str, Any]:
    return request.app.state.lifecycle.begin_shutdown()
