from __future__ import annotations

import argparse
import json
import unittest
from collections.abc import Mapping
from threading import Thread
from typing import Any
from urllib.request import urlopen
from wsgiref.simple_server import make_server

from meeting_intelligence_local_ai.app import application
from meeting_intelligence_local_ai.server import _mock_mode, _port


def request(path: str, method: str = "GET") -> tuple[str, dict[str, str], Mapping[str, Any]]:
    captured: dict[str, object] = {}

    def start_response(status: str, headers: list[tuple[str, str]]) -> None:
        captured["status"] = status
        captured["headers"] = dict(headers)

    chunks = application(
        {"PATH_INFO": path, "REQUEST_METHOD": method},
        start_response,
    )
    payload = json.loads(b"".join(chunks))
    return str(captured["status"]), dict(captured["headers"]), payload


class LocalAiApplicationTests(unittest.TestCase):
    def test_health_is_mock_and_not_cached(self) -> None:
        status, headers, payload = request("/health")

        self.assertEqual(status, "200 OK")
        self.assertEqual(headers["Cache-Control"], "no-store")
        self.assertEqual(
            payload,
            {
                "api_version": "v1alpha1",
                "mode": "mock",
                "service": "meeting-intelligence-local-ai",
                "status": "ok",
            },
        )

    def test_capabilities_make_unavailable_features_explicit(self) -> None:
        status, _, payload = request("/capabilities")

        self.assertEqual(status, "200 OK")
        self.assertEqual(payload["mode"], "mock")
        self.assertEqual(payload["models_loaded"], [])
        self.assertIs(payload["cuda_required"], False)
        self.assertIs(payload["network_ai_enabled"], False)
        self.assertTrue(
            all(
                capability == {"available": False, "reason": "bootstrap_mock"}
                for capability in payload["capabilities"].values()
            )
        )

    def test_unknown_path_returns_json_404(self) -> None:
        status, _, payload = request("/not-an-endpoint")

        self.assertEqual(status, "404 Not Found")
        self.assertEqual(payload, {"error": "not_found"})

    def test_unsupported_method_returns_405_and_allow_header(self) -> None:
        status, headers, payload = request("/health", method="POST")

        self.assertEqual(status, "405 Method Not Allowed")
        self.assertEqual(headers["Allow"], "GET")
        self.assertEqual(payload, {"error": "method_not_allowed"})

    def test_live_http_round_trip(self) -> None:
        server = make_server("127.0.0.1", 0, application)
        server.timeout = 2
        thread = Thread(target=server.handle_request)
        thread.start()

        try:
            with urlopen(f"http://127.0.0.1:{server.server_port}/health", timeout=2) as response:
                payload = json.load(response)
                self.assertEqual(response.status, 200)
                self.assertEqual(response.headers["Cache-Control"], "no-store")
                self.assertEqual(payload["mode"], "mock")
        finally:
            thread.join(timeout=3)
            server.server_close()

        self.assertFalse(thread.is_alive())

    def test_non_mock_modes_fail_closed(self) -> None:
        for value in ("local", "openai", "hybrid"):
            with (
                self.subTest(value=value),
                self.assertRaisesRegex(argparse.ArgumentTypeError, "only mock mode"),
            ):
                _mock_mode(value)

    def test_invalid_ports_fail_closed(self) -> None:
        for value in ("0", "65536"):
            with (
                self.subTest(value=value),
                self.assertRaisesRegex(argparse.ArgumentTypeError, "between 1 and 65535"),
            ):
                _port(value)


if __name__ == "__main__":
    unittest.main()
