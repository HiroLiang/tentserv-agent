from __future__ import annotations

import argparse
import json
from pathlib import Path

from tentgent_daemon.backends import create_backend
from tentgent_daemon.runtime.adapters import (
    load_adapter_record,
    validate_adapter_for_model,
)
from tentgent_daemon.runtime.chat import ChatRequest, Message, build_chat_plan


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Tentgent one-shot chat request from a stored model reference."
    )
    parser.add_argument("--model-ref", required=True, help="Stored Tentgent model ref")
    parser.add_argument(
        "--message",
        action="append",
        dest="messages",
        required=True,
        help=(
            "Message content. Repeat in order to build context. "
            "Use `role:content` for explicit roles such as `system:...`, "
            "`user:...`, or `assistant:...`."
        ),
    )
    parser.add_argument("--home", help="Optional Tentgent runtime home override")
    parser.add_argument("--max-tokens", type=int)
    parser.add_argument("--temperature", type=float)
    parser.add_argument("--adapter-ref", help="Optional compatible PEFT adapter ref")
    parser.add_argument(
        "--stream",
        action="store_true",
        help="Stream generated text to stdout as tokens arrive when the backend supports it.",
    )
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print the resolved runtime plan instead of running generation.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    request = ChatRequest(
        model_ref=args.model_ref,
        messages=tuple(_parse_message(content) for content in args.messages),
        max_tokens=args.max_tokens,
        temperature=args.temperature,
        adapter_ref=args.adapter_ref,
    )

    plan = build_chat_plan(
        request,
        home=Path(args.home).expanduser().resolve() if args.home else None,
    )

    if args.plan_only:
        print(
            json.dumps(
                {
                    "model_ref": plan.record.model_ref,
                    "short_ref": plan.record.short_ref,
                    "backend": str(plan.backend),
                    "load_path": str(plan.load_path),
                    "message_count": len(plan.request.messages),
                },
                indent=2,
            )
        )
        return 0

    adapter = None
    if request.adapter_ref:
        adapter = load_adapter_record(
            request.adapter_ref,
            home=Path(args.home).expanduser().resolve() if args.home else None,
        )
        validate_adapter_for_model(adapter, plan.record, plan.backend)
        request = ChatRequest(
            model_ref=request.model_ref,
            messages=request.messages,
            max_tokens=request.max_tokens,
            temperature=request.temperature,
            adapter_ref=adapter.adapter_ref,
        )

    backend = create_backend(plan.backend)
    backend.load(plan.record)
    backend.select_adapter(adapter)

    if args.stream:
        for chunk in backend.stream_generate(request):
            print(chunk, end="", flush=True)
        print()
        return 0

    result = backend.generate(request)
    print(result.text)
    return 0


def _parse_message(raw: str) -> Message:
    prefix, separator, remainder = raw.partition(":")
    role = prefix.strip().lower()
    if separator and role in {"system", "user", "assistant"}:
        content = remainder.strip()
        if not content:
            raise ValueError(f"message for role `{role}` must not be empty")
        return Message(role=role, content=content)
    return Message(role="user", content=raw)


if __name__ == "__main__":
    raise SystemExit(main())
