from __future__ import annotations

import io
import unittest
from http import HTTPStatus

from tentgent_daemon.server.http import TentgentRequestHandler


class JsonWriter(TentgentRequestHandler):
    def __init__(self) -> None:
        self.wfile = io.BytesIO()
        self.status: HTTPStatus | None = None
        self.headers: dict[str, str] = {}

    def send_response(self, code: HTTPStatus, message: str | None = None) -> None:
        self.status = code

    def send_header(self, keyword: str, value: str) -> None:
        self.headers[keyword] = value

    def end_headers(self) -> None:
        return


class ServerHttpTests(unittest.TestCase):
    def test_write_json_preserves_utf8_text(self) -> None:
        handler = JsonWriter()

        handler._write_json(HTTPStatus.OK, {"text": "你好，咕嚕", "stream": False})

        body = handler.wfile.getvalue()
        self.assertEqual(handler.status, HTTPStatus.OK)
        self.assertEqual(handler.headers["Content-Type"], "application/json; charset=utf-8")
        self.assertIn("你好，咕嚕".encode("utf-8"), body)
        self.assertNotIn(b"\\u4f60", body)


if __name__ == "__main__":
    unittest.main()
