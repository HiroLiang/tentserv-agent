from __future__ import annotations

import io
import json
import unittest
from http import HTTPStatus

from tentgent_daemon.runtime.adapters import AdapterBackendUnsupportedError
from tentgent_daemon.server.chat_api import handle_chat_request
from tentgent_daemon.server.chat_api import stream_preflight_error_response
from tentgent_daemon.server.http import TentgentRequestHandler
from tentgent_daemon.server.sse import delta_event, done_event, error_event


class JsonWriter(TentgentRequestHandler):
    def __init__(self, body: bytes = b"", session: object | None = None) -> None:
        self.rfile = io.BytesIO(body)
        self.wfile = io.BytesIO()
        self.status: HTTPStatus | None = None
        self.headers: dict[str, str] = {}
        if body:
            self.headers["Content-Length"] = str(len(body))
        self.server = type("FakeServer", (), {"session": session})()

    def send_response(self, code: HTTPStatus, message: str | None = None) -> None:
        self.status = code

    def send_header(self, keyword: str, value: str) -> None:
        self.headers[keyword] = value

    def end_headers(self) -> None:
        return


class StreamingSession:
    def __init__(
        self,
        chunks: list[str] | None = None,
        *,
        preflight_exc: Exception | None = None,
        runtime_exc: Exception | None = None,
    ) -> None:
        self.chunks = chunks or []
        self.preflight_exc = preflight_exc
        self.runtime_exc = runtime_exc
        self.requests: list[object] = []

    def stream_generate(self, request: object):
        self.requests.append(request)
        if self.preflight_exc is not None:
            raise self.preflight_exc

        def _iter():
            for chunk in self.chunks:
                yield chunk
            if self.runtime_exc is not None:
                raise self.runtime_exc

        return _iter()


class ServerHttpTests(unittest.TestCase):
    def test_write_json_preserves_utf8_text(self) -> None:
        handler = JsonWriter()

        handler._write_json(HTTPStatus.OK, {"text": "你好，咕嚕", "stream": False})

        body = handler.wfile.getvalue()
        self.assertEqual(handler.status, HTTPStatus.OK)
        self.assertEqual(handler.headers["Content-Type"], "application/json; charset=utf-8")
        self.assertIn("你好，咕嚕".encode("utf-8"), body)
        self.assertNotIn(b"\\u4f60", body)

    def test_sse_event_serializers_preserve_utf8_text(self) -> None:
        self.assertEqual(
            delta_event("你好"),
            'event: delta\ndata: {"delta":"你好"}\n\n'.encode("utf-8"),
        )
        self.assertEqual(
            done_event(),
            b'event: done\ndata: {"finish_reason":"stop"}\n\n',
        )
        self.assertEqual(
            error_event("runtime_error", "boom"),
            b'event: error\ndata: {"error":"runtime_error","message":"boom"}\n\n',
        )

    def test_write_sse_headers_and_event(self) -> None:
        handler = JsonWriter()

        handler._write_sse_headers()
        handler._write_sse_event("delta", {"delta": "咕嚕"})

        self.assertEqual(handler.status, HTTPStatus.OK)
        self.assertEqual(handler.headers["Content-Type"], "text/event-stream; charset=utf-8")
        self.assertEqual(handler.headers["Cache-Control"], "no-cache")
        self.assertEqual(
            handler.wfile.getvalue(),
            'event: delta\ndata: {"delta":"咕嚕"}\n\n'.encode("utf-8"),
        )

    def test_direct_handler_still_returns_json_error_for_stream_request(self) -> None:
        status, payload = handle_chat_request(
            b'{"messages":[{"role":"user","content":"hi"}],"stream":true}',
            session=object(),  # type: ignore[arg-type]
        )

        self.assertEqual(status, HTTPStatus.NOT_IMPLEMENTED)
        self.assertEqual(payload["error"], "stream_not_implemented")

    def test_direct_chat_rejects_daemon_session_fields(self) -> None:
        for field in ("session_ref", "max_session_messages"):
            with self.subTest(field=field):
                body = {
                    "messages": [{"role": "user", "content": "hi"}],
                    field: "session" if field == "session_ref" else 2,
                }

                status, payload = handle_chat_request(
                    json.dumps(body).encode("utf-8"),
                    session=object(),  # type: ignore[arg-type]
                )

                self.assertEqual(status, HTTPStatus.BAD_REQUEST)
                self.assertEqual(payload["error"], "session_context_unsupported")
                self.assertIn("daemon-only session chat fields", payload["message"])

    def test_stream_chat_rejects_daemon_session_fields_before_sse(self) -> None:
        body = (
            b'{"messages":[{"role":"user","content":"hi"}],'
            b'"session_ref":"session","stream":true}'
        )
        session = StreamingSession(["ok"])
        handler = JsonWriter(body, session=session)

        handler._handle_chat()

        self.assertEqual(handler.status, HTTPStatus.BAD_REQUEST)
        self.assertEqual(handler.headers["Content-Type"], "application/json; charset=utf-8")
        self.assertEqual(session.requests, [])
        payload = json.loads(handler.wfile.getvalue().decode("utf-8"))
        self.assertEqual(payload["error"], "session_context_unsupported")

    def test_stream_chat_writes_delta_and_done_events(self) -> None:
        body = (
            b'{"messages":[{"role":"user","content":"hi"}],'
            b'"max_tokens":5,"stream":true}'
        )
        session = StreamingSession(["你", "好"])
        handler = JsonWriter(body, session=session)

        handler._handle_chat()

        self.assertEqual(handler.status, HTTPStatus.OK)
        self.assertEqual(handler.headers["Content-Type"], "text/event-stream; charset=utf-8")
        self.assertEqual(handler.headers["Cache-Control"], "no-cache")
        self.assertEqual(len(session.requests), 1)
        self.assertEqual(
            handler.wfile.getvalue(),
            (
                'event: delta\ndata: {"delta":"你"}\n\n'
                'event: delta\ndata: {"delta":"好"}\n\n'
                'event: done\ndata: {"finish_reason":"stop"}\n\n'
            ).encode("utf-8"),
        )

    def test_stream_chat_passes_adapter_ref_to_session(self) -> None:
        body = (
            b'{"messages":[{"role":"user","content":"hi"}],'
            b'"adapter_ref":"adapter-ref","stream":true}'
        )
        session = StreamingSession(["ok"])
        handler = JsonWriter(body, session=session)

        handler._handle_chat()

        self.assertEqual(handler.status, HTTPStatus.OK)
        self.assertEqual(len(session.requests), 1)
        self.assertEqual(session.requests[0].adapter_ref, "adapter-ref")
        self.assertEqual(
            handler.wfile.getvalue(),
            (
                'event: delta\ndata: {"delta":"ok"}\n\n'
                'event: done\ndata: {"finish_reason":"stop"}\n\n'
            ).encode("utf-8"),
        )

    def test_stream_preflight_error_returns_normal_json(self) -> None:
        body = b'{"messages":[{"role":"user","content":"hi"}],"stream":true}'
        session = StreamingSession(preflight_exc=NotImplementedError("no stream"))
        handler = JsonWriter(body, session=session)

        handler._handle_chat()

        self.assertEqual(handler.status, HTTPStatus.NOT_IMPLEMENTED)
        self.assertEqual(handler.headers["Content-Type"], "application/json; charset=utf-8")
        payload = json.loads(handler.wfile.getvalue().decode("utf-8"))
        self.assertEqual(payload["error"], "stream_not_implemented")
        self.assertEqual(payload["message"], "no stream")

    def test_stream_preflight_preserves_adapter_error_mapping(self) -> None:
        status, payload = stream_preflight_error_response(
            AdapterBackendUnsupportedError("adapter backend mismatch")
        )

        self.assertEqual(status, HTTPStatus.NOT_IMPLEMENTED)
        self.assertEqual(payload["error"], "adapter_backend_unsupported")

    def test_stream_runtime_error_writes_sse_error_event(self) -> None:
        body = b'{"messages":[{"role":"user","content":"hi"}],"stream":true}'
        session = StreamingSession(["hi"], runtime_exc=RuntimeError("boom"))
        handler = JsonWriter(body, session=session)

        handler._handle_chat()

        self.assertEqual(handler.status, HTTPStatus.OK)
        self.assertEqual(handler.headers["Content-Type"], "text/event-stream; charset=utf-8")
        self.assertEqual(
            handler.wfile.getvalue(),
            (
                'event: delta\ndata: {"delta":"hi"}\n\n'
                'event: error\ndata: {"error":"runtime_error","message":"boom"}\n\n'
            ).encode("utf-8"),
        )

    def test_invalid_stream_flag_returns_normal_json_request_error(self) -> None:
        status, payload = handle_chat_request(
            b'{"messages":[{"role":"user","content":"hi"}],"stream":"yes"}',
            session=object(),  # type: ignore[arg-type]
        )

        self.assertEqual(status, HTTPStatus.BAD_REQUEST)
        self.assertEqual(payload["error"], "invalid_request")
        self.assertIn("`stream` must be a boolean", payload["message"])


if __name__ == "__main__":
    unittest.main()
