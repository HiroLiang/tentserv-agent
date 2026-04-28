"""Dataset schema validation, rendering, and provider helpers."""

from .provider import (
    DATASET_PROVIDER_SYSTEM_PROMPT,
    DatasetJsonlGenerationRequest,
    DatasetJsonlGenerationResponse,
    DatasetProviderCallRequest,
    DatasetProviderCallResponse,
    DatasetProviderError,
    DatasetProviderParseError,
    DatasetProviderRequestError,
    ParsedDatasetJsonl,
    call_dataset_provider,
    generate_dataset_jsonl,
    parse_dataset_jsonl,
    records_to_jsonl,
)
from .synth import (
    DATASET_SYNTH_MANIFEST_SCHEMA,
    DATASET_TEMPLATE_VERSION,
    DatasetSynthPackageOutcome,
    build_dataset_generation_prompt,
    write_dataset_synth_package,
)

__all__ = [
    "DATASET_PROVIDER_SYSTEM_PROMPT",
    "DATASET_SYNTH_MANIFEST_SCHEMA",
    "DATASET_TEMPLATE_VERSION",
    "DatasetJsonlGenerationRequest",
    "DatasetJsonlGenerationResponse",
    "DatasetProviderCallRequest",
    "DatasetProviderCallResponse",
    "DatasetProviderError",
    "DatasetProviderParseError",
    "DatasetProviderRequestError",
    "DatasetSynthPackageOutcome",
    "ParsedDatasetJsonl",
    "build_dataset_generation_prompt",
    "call_dataset_provider",
    "generate_dataset_jsonl",
    "parse_dataset_jsonl",
    "records_to_jsonl",
    "write_dataset_synth_package",
]
