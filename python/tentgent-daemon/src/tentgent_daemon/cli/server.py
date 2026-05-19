from __future__ import annotations

import argparse
from pathlib import Path

from tentgent_daemon.server.config import ServerConfig
from tentgent_daemon.server.http import serve
from tentgent_daemon.server.session import RuntimeSession


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the Tentgent long-lived server skeleton."
    )
    parser.add_argument("--server-ref", required=True, help="Tentgent server ref")
    parser.add_argument(
        "--runtime-kind",
        choices=("local", "cloud"),
        default="local",
        help="Server runtime kind.",
    )
    parser.add_argument(
        "--capability",
        choices=("chat", "embedding", "rerank"),
        default="chat",
        help="Endpoint family served by this process.",
    )
    parser.add_argument("--model-ref", help="Stored Tentgent model ref")
    parser.add_argument("--provider", choices=("openai", "anthropic"), help="Cloud provider")
    parser.add_argument("--provider-model", help="Cloud provider model name")
    parser.add_argument("--host", required=True, help="HTTP bind host")
    parser.add_argument("--port", required=True, type=int, help="HTTP bind port")
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument(
        "--lazy-load",
        action="store_true",
        help="Delay model loading until the first request arrives.",
    )
    parser.add_argument(
        "--idle-seconds",
        type=int,
        help="Auto-release the loaded model after N idle seconds.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runtime_kind == "local" and not args.model_ref:
        raise SystemExit("--model-ref is required for local server runtimes")
    if args.runtime_kind == "cloud" and (not args.provider or not args.provider_model):
        raise SystemExit("--provider and --provider-model are required for cloud server runtimes")
    if args.runtime_kind == "cloud" and args.capability != "chat":
        raise SystemExit("cloud server runtimes support only --capability chat")
    config = ServerConfig(
        server_ref=args.server_ref,
        runtime_kind=args.runtime_kind,
        capability=args.capability,
        model_ref=args.model_ref,
        provider=args.provider,
        provider_model=args.provider_model,
        host=args.host,
        port=args.port,
        home=Path(args.home).expanduser().resolve() if args.home else None,
        lazy_load=args.lazy_load,
        idle_seconds=args.idle_seconds,
    )
    session = RuntimeSession(config)
    if not config.lazy_load:
        session.ensure_loaded()
    return serve(config, session)


if __name__ == "__main__":
    raise SystemExit(main())
