from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.audio_speech import (
    AudioSpeechBackendModel,
    AudioSpeechModelKind,
    AudioSpeechRequest,
    AudioSpeechResult,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class AudioSpeechInferenceRequest:
    model_kind: AudioSpeechModelKind
    model: ModelRecord
    speech: AudioSpeechRequest


class AudioSpeechTask(InferenceTask[AudioSpeechInferenceRequest, AudioSpeechResult]):
    def __init__(
        self,
        *,
        task_ref: str,
        request: AudioSpeechInferenceRequest,
        resources: ResourceManager[AudioSpeechBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.AUDIO_SPEECH,
            request=request,
        )
        self._resources = resources

    def execute(self) -> AudioSpeechResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as audio_model:
            return audio_model.synthesize_speech(self.request.speech)
