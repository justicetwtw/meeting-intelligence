"""WSGI endpoints for the mock-only local AI service."""

from __future__ import annotations

import json
from collections.abc import Callable, Iterable, Mapping
from typing import Any

SERVICE_NAME = "meeting-intelligence-local-ai"
API_VERSION = "v1alpha1"

StartResponse = Callable[[str, list[tuple[str, str]]], Any]
Payload = Mapping[str, object]


def _health_payload() -> Payload:
    return {
        "api_version": API_VERSION,
        "mode": "mock",
        "service": SERVICE_NAME,
        "status": "ok",
    }


def _capabilities_payload() -> Payload:
    unavailable = {"available": False, "reason": "bootstrap_mock"}
    return {
        "api_version": API_VERSION,
        "capabilities": {
            "meeting_intelligence": unavailable,
            "speaker_diarization": unavailable,
            "speaker_identity": unavailable,
            "transcription": unavailable,
        },
        "cuda_required": False,
        "mode": "mock",
        "models_loaded": [],
        "network_ai_enabled": False,
        "service": SERVICE_NAME,
    }


def _json_response(
    start_response: StartResponse,
    status: str,
    payload: Payload,
    *,
    extra_headers: Iterable[tuple[str, str]] = (),
) -> list[bytes]:
    body = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
    headers = [
        ("Cache-Control", "no-store"),
        ("Content-Length", str(len(body))),
        ("Content-Type", "application/json; charset=utf-8"),
        *extra_headers,
    ]
    start_response(status, headers)
    return [body]


def application(environ: Mapping[str, object], start_response: StartResponse) -> list[bytes]:
    """Serve the two bootstrap endpoints without external dependencies or calls."""

    method = str(environ.get("REQUEST_METHOD", "GET")).upper()
    path = str(environ.get("PATH_INFO", "/"))

    if method != "GET":
        return _json_response(
            start_response,
            "405 Method Not Allowed",
            {"error": "method_not_allowed"},
            extra_headers=(("Allow", "GET"),),
        )

    if path == "/health":
        return _json_response(start_response, "200 OK", _health_payload())
    if path == "/capabilities":
        return _json_response(start_response, "200 OK", _capabilities_payload())

    return _json_response(start_response, "404 Not Found", {"error": "not_found"})
