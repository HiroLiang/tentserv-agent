from __future__ import annotations

import json
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

from .chat_api import (
    SessionContextUnsupportedError,
    decode_chat_request,
    handle_parsed_chat_request,
    stream_preflight_error_response,
)
from .config import ServerConfig
from .embedding_api import decode_embedding_request, handle_parsed_embedding_request
from .health import build_health_payload
from .session import ChatRequestPayload, RuntimeSession
from .sse import SSE_CACHE_CONTROL, SSE_CONTENT_TYPE, encode_sse_event


class TentgentServer(ThreadingHTTPServer):
    def __init__(self, config: ServerConfig, session: RuntimeSession) -> None:
        super().__init__((config.host, config.port), TentgentRequestHandler)
        self.config = config
        self.session = session


class TentgentRequestHandler(BaseHTTPRequestHandler):
    server_version = "TentgentServer/0.1"

    @property
    def config(self) -> ServerConfig:
        return self.server.config  # type: ignore[attr-defined]

    @property
    def session(self) -> RuntimeSession:
        return self.server.session  # type: ignore[attr-defined]

    def do_GET(self) -> None:
        if self.path == "/healthz":
            self._write_json(HTTPStatus.OK, build_health_payload(self.config, self.session))
            return

        self._write_json(
            HTTPStatus.NOT_FOUND,
            {
                "error": "not_found",
                "message": f"no route exists for GET {self.path}",
            },
        )

    def do_POST(self) -> None:
        if self.path == "/v1/chat":
            if not self.config.is_chat:
                self._write_json(
                    HTTPStatus.BAD_REQUEST,
                    {
                        "error": "unsupported_target",
                        "message": (
                            f"server capability `{self.config.capability}` "
                            "does not serve POST /v1/chat"
                        ),
                    },
                )
                return
            self._handle_chat()
            return
        if self.path == "/v1/embeddings":
            if not self.config.is_embedding:
                self._write_json(
                    HTTPStatus.BAD_REQUEST,
                    {
                        "error": "unsupported_target",
                        "message": (
                            f"server capability `{self.config.capability}` "
                            "does not serve POST /v1/embeddings"
                        ),
                    },
                )
                return
            self._handle_embeddings()
            return

        self._write_json(
            HTTPStatus.NOT_FOUND,
            {
                "error": "not_found",
                "message": f"no route exists for POST {self.path}",
            },
        )

    def log_message(self, format: str, *args: Any) -> None:
        return

    def _handle_chat(self) -> None:
        content_length = self.headers.get("Content-Length")
        if content_length is None:
            self._write_json(
                HTTPStatus.LENGTH_REQUIRED,
                {
                    "error": "length_required",
                    "message": "Content-Length is required for POST /v1/chat",
                },
            )
            return

        try:
            body_length = int(content_length)
        except ValueError:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {
                    "error": "invalid_content_length",
                    "message": "Content-Length must be an integer",
                },
            )
            return

        raw_body = self.rfile.read(body_length)
        try:
            request = decode_chat_request(raw_body)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {"error": "invalid_json", "message": str(exc)},
            )
            return
        except SessionContextUnsupportedError as exc:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {"error": "session_context_unsupported", "message": str(exc)},
            )
            return
        except ValueError as exc:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {"error": "invalid_request", "message": str(exc)},
            )
            return

        if request.stream:
            self._handle_chat_stream(request)
            return

        status, payload = handle_parsed_chat_request(request, self.session)
        self._write_json(status, payload)

    def _handle_chat_stream(self, request: ChatRequestPayload) -> None:
        try:
            chunks = self.session.stream_generate(request)
        except Exception as exc:  # pragma: no cover - mapped by tests via fake sessions.
            status, payload = stream_preflight_error_response(exc)
            self._write_json(status, payload)
            return

        self._write_sse_headers()
        try:
            for chunk in chunks:
                self._write_sse_event("delta", {"delta": chunk})
            self._write_sse_event("done", {"finish_reason": "stop"})
        except (BrokenPipeError, ConnectionResetError):  # pragma: no cover - client abort.
            close = getattr(chunks, "close", None)
            if callable(close):
                close()
        except Exception as exc:  # pragma: no cover - backend runtime surface.
            self._write_sse_event(
                "error",
                {
                    "error": "runtime_error",
                    "message": str(exc),
                },
            )

    def _handle_embeddings(self) -> None:
        content_length = self.headers.get("Content-Length")
        if content_length is None:
            self._write_json(
                HTTPStatus.LENGTH_REQUIRED,
                {
                    "error": "length_required",
                    "message": "Content-Length is required for POST /v1/embeddings",
                },
            )
            return

        try:
            body_length = int(content_length)
        except ValueError:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {
                    "error": "invalid_content_length",
                    "message": "Content-Length must be an integer",
                },
            )
            return

        raw_body = self.rfile.read(body_length)
        try:
            request = decode_embedding_request(raw_body)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {"error": "invalid_json", "message": str(exc)},
            )
            return
        except ValueError as exc:
            self._write_json(
                HTTPStatus.BAD_REQUEST,
                {"error": "invalid_request", "message": str(exc)},
            )
            return

        status, payload = handle_parsed_embedding_request(request, self.session)
        self._write_json(status, payload)

    def _write_json(self, status: HTTPStatus, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _write_sse_headers(self, status: HTTPStatus = HTTPStatus.OK) -> None:
        self.send_response(status)
        self.send_header("Content-Type", SSE_CONTENT_TYPE)
        self.send_header("Cache-Control", SSE_CACHE_CONTROL)
        self.end_headers()

    def _write_sse_event(self, event: str, data: dict[str, Any]) -> None:
        self.wfile.write(encode_sse_event(event, data))
        self.wfile.flush()


def serve(config: ServerConfig, session: RuntimeSession) -> int:
    server = TentgentServer(config, session)
    try:
        print(
            f"Tentgent server skeleton listening on http://{config.host}:{config.port} "
            f"for {config.runtime_kind} {config.capability} runtime {config.runtime_label}",
            flush=True,
        )
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()

    return 0
