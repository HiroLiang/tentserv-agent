from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Request
from tentgent.runtime.server.preload import preload_bound_model

router = APIRouter(prefix="/v1/lifecycle")


@router.post("/preload")
async def preload(request: Request) -> dict[str, Any]:
    return await preload_bound_model(request)


@router.post("/shutdown")
def shutdown(request: Request) -> dict[str, Any]:
    return request.app.state.lifecycle.begin_shutdown()
