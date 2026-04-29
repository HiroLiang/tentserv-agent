# Plan Archive

Use this directory for completed plans that are no longer the active execution track.

## Scope

- Keep historical plans available when implementation history matters.
- Remove completed plans from the active plan surface in the parent folder.
- Keep the active `docs/plans/` directory focused on unfinished or future work.

## Routing Rule

- Start in the parent [README.md](../README.md) for active plans.
- Read this archive only when you need:
  - implementation history
  - completed slice order
  - rationale behind earlier runtime or server decisions

## Archived Plans

- [runtime-chat-mvp.md](./runtime-chat-mvp.md)
  Completed foundation for one-shot chat and backend routing.
- [server-runtime-mvp.md](./server-runtime-mvp.md)
  Completed first server lifecycle and management surface.
- [lora-server-mvp.md](./lora-server-mvp.md)
  Completed adapter, dataset, LoRA training, and request-time adapter execution milestone.
- [cloud-dataset-mvp.md](./cloud-dataset-mvp.md)
  Completed OpenAI/Claude-assisted dataset validation, prompt-template generation, synthesis, debugging, and evaluation.
- [cloud-provider-server-mvp.md](./cloud-provider-server-mvp.md)
  Completed OpenAI and Claude cloud provider server routing through local `tentgent server` chat.
- [http-chat-streaming-mvp.md](./http-chat-streaming-mvp.md)
  Completed Server-Sent Events streaming for local base-model, local adapter, and cloud provider chat.
