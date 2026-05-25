from __future__ import annotations

import argparse
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
        "--idle-keep-alive-seconds",
        default=300.0,
        type=float,
        help=(
            "Idle seconds before the runtime begins graceful shutdown. "
            "Use a negative value to keep the process alive until external shutdown."
        ),
    )
    parser.add_argument(
        "--model-idle-timeout-seconds",
        default=0.0,
        type=float,
        help=(
            "Idle seconds before an unused loaded model resource is released. "
            "Use a negative value to keep loaded model resources until shutdown."
        ),
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
    return parser.parse_args(argv)


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
            idle_keep_alive_seconds=args.idle_keep_alive_seconds,
            model_idle_timeout_seconds=args.model_idle_timeout_seconds,
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
