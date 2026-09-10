# Image Workflows

Use an `image-generation` model compatible with the requested workflow. Prepare `tentgent runtime bootstrap --profile local-model` first. Diffusers and Apple Silicon MFLUX paths have different model requirements; see the [fixture guide](../../model-fixtures.md).

| Goal | Command | Example and parameters |
| --- | --- | --- |
| Create an image from text | `image generate` | [Generation](./generate.md) |
| Restyle an existing image | `image transform` | [Transform](./edit.md#transform) |
| Repaint a masked region | `image inpaint` | [Inpaint](./edit.md#inpaint) |
| Guide with a typed control image | `image control` | [Control](./edit.md#control) |

Each guide includes daemon request fields and result downloads. CLI commands read local files and require a new output path; daemon edit endpoints upload bytes. See [file rules](../README.md#file-and-http-media-rules).

[Image adapters](../../adapters.md) are imported separately. LoRA selection uses `--adapter-ref`; ControlNet uses `--control-ref`. MLX inpainting needs a Flux Fill-compatible model. Control images must already contain the requested representation; preprocessing is not automatic.
