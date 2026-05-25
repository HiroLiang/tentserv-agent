from __future__ import annotations

from abc import ABC

from tentgent.runtime.task.task import RequestT, ResultT, RuntimeTask


class InferenceTask(RuntimeTask[RequestT, ResultT], ABC):
    pass
