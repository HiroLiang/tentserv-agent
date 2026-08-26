from __future__ import annotations

import argparse
import math
import os
from collections.abc import Sequence
from pathlib import Path

import uvicorn

from tentgent.runtime.server.app import create_app
from tentgent.runtime.server.lifecycle import RuntimeCapability, RuntimeServerConfig


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the Tentgent model runtime daemon."
    )
    parser.add_argument("--host", default="127.0.0.1", help="HTTP bind host.")
    parser.add_argument("--port", required=True, type=int, help="HTTP bind port.")
    parser.add_argument("--server-ref", help="Optional Rust-owned server reference.")
    parser.add_argument("--model-ref", help="Optional Rust-owned model reference.")
    parser.add_argument("--home", help="Optional Tentgent runtime home for model-bound servers.")
    parser.add_argument(
        "--capability",
        choices=tuple(capability.value for capability in RuntimeCapability),
        default=RuntimeCapability.CHAT.value,
        help="Endpoint family served by this runtime process.",
    )
    parser.add_argument(
        "--log-level",
        choices=("critical", "error", "warning", "info", "debug", "trace"),
        default="info",
        help="Uvicorn log level.",
    )
    parser.add_argument(
        "--access-log",
        action="store_true",
        help="Enable per-request access logging.",
    )
    parser.add_argument(
        "--lazy-load",
        action="store_true",
        help="Delay model loading until the first request.",
    )
    parser.add_argument(
        "--runtime-idle-seconds",
        type=float,
        help="Idle seconds before the runtime begins graceful shutdown (default: 300).",
    )
    parser.add_argument(
        "--idle-keep-alive-seconds",
        type=float,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--model-idle-seconds",
        type=float,
        help="Idle seconds before an unused loaded model is released (default: 0).",
    )
    parser.add_argument(
        "--model-idle-timeout-seconds",
        type=float,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--closing-grace-seconds",
        default=2.0,
        type=float,
        help="Seconds to report closing before requesting server shutdown.",
    )
    parser.add_argument(
        "--task-poll-interval-seconds",
        default=0.5,
        type=float,
        help="Seconds between task cleanup and idle lifecycle polls.",
    )
    args = parser.parse_args(argv)
    args.runtime_idle_seconds = _resolve_timeout_alias(
        parser,
        canonical_name="--runtime-idle-seconds",
        canonical_value=args.runtime_idle_seconds,
        legacy_name="--idle-keep-alive-seconds",
        legacy_value=args.idle_keep_alive_seconds,
        default=300.0,
    )
    args.model_idle_seconds = _resolve_timeout_alias(
        parser,
        canonical_name="--model-idle-seconds",
        canonical_value=args.model_idle_seconds,
        legacy_name="--model-idle-timeout-seconds",
        legacy_value=args.model_idle_timeout_seconds,
        default=0.0,
    )
    if args.model_idle_seconds > args.runtime_idle_seconds:
        parser.error(
            "--model-idle-seconds must be less than or equal to "
            "--runtime-idle-seconds"
        )
    return args


def _resolve_timeout_alias(
    parser: argparse.ArgumentParser,
    *,
    canonical_name: str,
    canonical_value: float | None,
    legacy_name: str,
    legacy_value: float | None,
    default: float,
) -> float:
    if (
        canonical_value is not None
        and legacy_value is not None
        and canonical_value != legacy_value
    ):
        parser.error(
            f"{canonical_name} and deprecated {legacy_name} must match when both are set"
        )
    value = canonical_value if canonical_value is not None else legacy_value
    value = default if value is None else value
    if not math.isfinite(value) or value < 0:
        parser.error(f"{canonical_name} must be finite and non-negative")
    return value


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    server_holder: dict[str, uvicorn.Server] = {}
    home = Path(args.home).expanduser().resolve() if args.home else None
    if home is not None:
        os.environ["TENTGENT_HOME"] = str(home)

    def request_shutdown() -> None:
        server = server_holder.get("server")
        if server is not None:
            server.should_exit = True

    app = create_app(
        RuntimeServerConfig(
            host=args.host,
            port=args.port,
            capability=RuntimeCapability(args.capability),
            server_ref=args.server_ref,
            model_ref=args.model_ref,
            home=home,
            lazy_load=args.lazy_load,
            runtime_idle_seconds=args.runtime_idle_seconds,
            model_idle_seconds=args.model_idle_seconds,
            closing_grace_seconds=args.closing_grace_seconds,
            task_poll_interval_seconds=args.task_poll_interval_seconds,
        ),
        request_shutdown=request_shutdown,
    )
    server = uvicorn.Server(
        uvicorn.Config(
            app,
            host=args.host,
            port=args.port,
            log_level=args.log_level,
            access_log=args.access_log,
        )
    )
    server_holder["server"] = server
    server.run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
