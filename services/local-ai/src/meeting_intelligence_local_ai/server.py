"""Command-line entry point for the mock-only WSGI service."""

from __future__ import annotations

import argparse
import os
from wsgiref.simple_server import WSGIRequestHandler, make_server

from .app import application


class QuietRequestHandler(WSGIRequestHandler):
    """Avoid logging full request targets, which may later contain sensitive data."""

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002
        return


def _port(value: str) -> int:
    port = int(value)
    if not 1 <= port <= 65535:
        raise argparse.ArgumentTypeError("port must be between 1 and 65535")
    return port


def _mock_mode(value: str) -> str:
    if value != "mock":
        raise argparse.ArgumentTypeError("only mock mode is available in the bootstrap service")
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default=os.getenv("MI_LOCAL_AI_HOST", "127.0.0.1"))
    parser.add_argument(
        "--port",
        default=os.getenv("MI_LOCAL_AI_PORT", "8091"),
        type=_port,
    )
    parser.add_argument(
        "--mode",
        default=os.getenv("MI_LOCAL_AI_MODE", "mock"),
        type=_mock_mode,
    )
    return parser


def main() -> None:
    args = build_parser().parse_args()
    with make_server(
        args.host,
        args.port,
        application,
        handler_class=QuietRequestHandler,
    ) as server:
        print(f"Local AI mock service listening on http://{args.host}:{args.port}", flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
