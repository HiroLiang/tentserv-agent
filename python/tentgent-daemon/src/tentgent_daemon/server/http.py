from __future__ import annotations

import json
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

from .chat_api import handle_chat_request
from .config import ServerConfig
from .health import build_health_payload
from .session import RuntimeSession


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
            self._handle_chat()
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
        status, payload = handle_chat_request(raw_body, self.session)
        self._write_json(status, payload)

    def _write_json(self, status: HTTPStatus, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def serve(config: ServerConfig, session: RuntimeSession) -> int:
    server = TentgentServer(config, session)
    try:
        print(
            f"Tentgent server skeleton listening on http://{config.host}:{config.port} "
            f"for {config.runtime_kind} runtime {config.runtime_label}",
            flush=True,
        )
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()

    return 0
