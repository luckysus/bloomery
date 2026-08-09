from __future__ import annotations

import json
from typing import Any, BinaryIO, Mapping

PROTOCOL_VERSION = "1.0"
MAX_FRAME_BYTES = 8 * 1024 * 1024
_MAX_HEADER_BYTES = 4096


class FrameError(ValueError):
    """Raised when a worker frame or request envelope is invalid."""


def encode_frame(message: Mapping[str, Any]) -> bytes:
    body = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(body) > MAX_FRAME_BYTES:
        raise FrameError(
            f"frame body is too large: {len(body)} bytes (maximum {MAX_FRAME_BYTES})"
        )
    return f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body


def read_frame(stream: BinaryIO) -> dict[str, Any] | None:
    header = bytearray()
    while not header.endswith(b"\r\n\r\n"):
        byte = stream.read(1)
        if not byte:
            if not header:
                return None
            raise FrameError("unexpected end of worker frame")
        header.extend(byte)
        if len(header) > _MAX_HEADER_BYTES:
            raise FrameError("invalid frame header: header is too large")

    try:
        header_text = bytes(header).decode("utf-8")
    except UnicodeDecodeError as error:
        raise FrameError("invalid frame header: header is not UTF-8") from error

    content_length: int | None = None
    for line in header_text.split("\r\n"):
        if line.startswith("Content-Length:"):
            if content_length is not None:
                raise FrameError("invalid frame header: duplicate Content-Length")
            try:
                content_length = int(line.split(":", 1)[1].strip())
            except ValueError as error:
                raise FrameError("invalid frame header: Content-Length must be an integer") from error
    if content_length is None:
        raise FrameError("invalid frame header: Content-Length is required")
    if content_length < 0 or content_length > MAX_FRAME_BYTES:
        raise FrameError(
            f"frame body is too large: {content_length} bytes (maximum {MAX_FRAME_BYTES})"
        )

    body = stream.read(content_length)
    if len(body) != content_length:
        raise FrameError("unexpected end of worker frame")
    try:
        message = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise FrameError(f"invalid JSON frame: {error}") from error
    if not isinstance(message, dict):
        raise FrameError("invalid JSON frame: top-level value must be an object")
    return message


def parse_request(message: Mapping[str, Any]) -> dict[str, Any]:
    if message.get("jsonrpc") != "2.0":
        raise FrameError("invalid worker request: jsonrpc must be 2.0")
    if message.get("protocol_version") != PROTOCOL_VERSION:
        raise FrameError(
            f"invalid worker request: protocol_version must be {PROTOCOL_VERSION}"
        )
    request_id = message.get("id")
    if not isinstance(request_id, str) or not request_id.strip():
        raise FrameError("invalid worker request: id must not be empty")
    method = message.get("method")
    if not isinstance(method, str) or not method.strip():
        raise FrameError("invalid worker request: method must not be empty")
    params = message.get("params", {})
    if not isinstance(params, dict):
        raise FrameError("invalid worker request: params must be an object")
    return dict(message)
