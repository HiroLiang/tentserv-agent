from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest.mock import MagicMock

from tentgent.runtime.cli.daemon import parse_args
from tentgent.runtime.server.lifecycle import (
    RuntimeCapability,
    RuntimeLifecycleState,
    RuntimeServerConfig,
)


class ImageLoadModeTests(unittest.TestCase):
    def test_cli_requires_explicit_lazy_even_without_bound_model(self) -> None:
        base = ["--port", "8780", "--capability", "image-generation"]
        for extra in [[], ["--model-ref", "image-model"]]:
            with self.subTest(extra=extra):
                with self.assertRaises(SystemExit) as raised:
                    parse_args(base + extra)
                self.assertEqual(raised.exception.code, 2)
                self.assertTrue(parse_args(base + extra + ["--lazy-load"]).lazy_load)

    def test_config_rejects_eager_image_and_allows_non_image_eager(self) -> None:
        for model_ref in [None, "image-model"]:
            with self.subTest(model_ref=model_ref):
                with self.assertRaisesRegex(ValueError, "requires --lazy-load"):
                    RuntimeServerConfig(
                        host="127.0.0.1", port=0,
                        capability=RuntimeCapability.IMAGE_GENERATION,
                        model_ref=model_ref, lazy_load=False,
                    )
        for capability in RuntimeCapability:
            if capability != RuntimeCapability.IMAGE_GENERATION:
                self.assertFalse(RuntimeServerConfig(
                    host="127.0.0.1", port=0, capability=capability, lazy_load=False,
                ).lazy_load)


class ImageLazyStartupTests(unittest.IsolatedAsyncioTestCase):
    async def test_lazy_start_does_not_acquire_or_prepare_any_image_workflow(self) -> None:
        tasks, resources = MagicMock(), MagicMock()
        lifecycle = RuntimeLifecycleState(
            config=RuntimeServerConfig(
                host="127.0.0.1", port=0,
                capability=RuntimeCapability.IMAGE_GENERATION,
                model_ref="image-model", lazy_load=True,
            ),
            task_manager=tasks, resource_manager=resources,
            bound_model=SimpleNamespace(model=object()),
        )
        await lifecycle.start()
        try:
            resources.lease_model.assert_not_called()
            tasks.touch_activity.assert_called_once()
        finally:
            await lifecycle.stop()
