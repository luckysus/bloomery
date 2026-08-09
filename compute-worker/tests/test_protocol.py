import io

import pytest

from bloomery_worker.protocol import (
    PROTOCOL_VERSION,
    FrameError,
    encode_frame,
    parse_request,
    read_frame,
)


def test_content_length_frames_round_trip_unicode_and_newlines() -> None:
    message = {
        "jsonrpc": "2.0",
        "protocol_version": PROTOCOL_VERSION,
        "id": "task-1",
        "method": "submit",
        "params": {"label": "连铸\n温度", "path": r"F:\数据\input.json"},
    }

    encoded = encode_frame(message)
    assert encoded.startswith(b"Content-Length: ")
    assert read_frame(io.BytesIO(encoded)) == message


def test_request_validation_rejects_missing_version_and_method() -> None:
    valid = parse_request(
        {
            "jsonrpc": "2.0",
            "protocol_version": PROTOCOL_VERSION,
            "id": "hello-1",
            "method": "hello",
            "params": {},
        }
    )
    assert valid["method"] == "hello"

    with pytest.raises(FrameError, match="protocol_version"):
        parse_request({"jsonrpc": "2.0", "id": "hello-1", "method": "hello"})

    with pytest.raises(FrameError, match="method"):
        parse_request(
            {
                "jsonrpc": "2.0",
                "protocol_version": PROTOCOL_VERSION,
                "id": "hello-1",
                "method": " ",
            }
        )


def test_malformed_or_truncated_frames_are_rejected() -> None:
    with pytest.raises(FrameError, match="Content-Length"):
        read_frame(io.BytesIO(b"Content-Length: nope\r\n\r\n{}"))

    with pytest.raises(FrameError, match="unexpected end"):
        read_frame(io.BytesIO(b"Content-Length: 5\r\n\r\n{}"))
