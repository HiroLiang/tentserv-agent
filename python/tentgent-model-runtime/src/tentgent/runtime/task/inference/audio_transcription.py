from __future__ import annotations

from dataclasses import dataclass

from tentgent.runtime.backends.audio_transcription import (
    AudioTranscriptionBackendModel,
    AudioTranscriptionModelKind,
    AudioTranscriptionRequest,
    AudioTranscriptionResult,
)
from tentgent.runtime.backends.records import ModelRecord
from tentgent.runtime.backends.resource_manager import ResourceManager
from tentgent.runtime.task.task import TaskKind

from .inference_task import InferenceTask


@dataclass(frozen=True, slots=True)
class AudioTranscriptionInferenceRequest:
    model_kind: AudioTranscriptionModelKind
    model: ModelRecord
    transcription: AudioTranscriptionRequest


class AudioTranscriptionTask(
    InferenceTask[AudioTranscriptionInferenceRequest, AudioTranscriptionResult],
):
    def __init__(
        self,
        *,
        task_ref: str,
        request: AudioTranscriptionInferenceRequest,
        resources: ResourceManager[AudioTranscriptionBackendModel],
    ) -> None:
        super().__init__(
            task_ref=task_ref,
            kind=TaskKind.AUDIO_TRANSCRIPTION,
            request=request,
        )
        self._resources = resources

    def execute(self) -> AudioTranscriptionResult:
        with self._resources.lease_model(
            self.request.model_kind,
            self.request.model,
        ) as audio_model:
            return audio_model.transcribe(self.request.transcription)
