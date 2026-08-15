use bloomery::compute::protocol::{
    encode_frame, parse_request, read_frame, write_frame, FrameError, WorkerRequest,
    PROTOCOL_VERSION,
};
use serde_json::json;
use std::io::{Cursor, Write};

#[test]
fn content_length_frames_round_trip_unicode_and_embedded_newlines() {
    let message = json!({
        "jsonrpc": "2.0",
        "protocol_version": PROTOCOL_VERSION,
        "id": "task-1",
        "method": "submit",
        "params": {"label": "连铸\n温度", "path": "F:\\数据\\input.json"}
    });

    let encoded = encode_frame(&message).expect("encode frame");
    assert!(encoded.starts_with(b"Content-Length: "));
    assert!(encoded.windows(4).any(|window| window == b"\r\n\r\n"));

    let decoded = read_frame(&mut Cursor::new(encoded))
        .expect("read frame")
        .expect("frame should exist");
    assert_eq!(decoded, message);
}

#[test]
fn request_validation_requires_protocol_version_and_non_empty_method() {
    let valid = parse_request(json!({
        "jsonrpc": "2.0",
        "protocol_version": PROTOCOL_VERSION,
        "id": "hello-1",
        "method": "hello",
        "params": {}
    }))
    .expect("valid request");
    assert_eq!(valid.method, "hello");

    let missing_version = parse_request(json!({
        "jsonrpc": "2.0",
        "id": "hello-1",
        "method": "hello",
        "params": {}
    }))
    .expect_err("missing protocol version must be rejected");
    assert!(missing_version.to_string().contains("protocol_version"));

    let missing_method = parse_request(json!({
        "jsonrpc": "2.0",
        "protocol_version": PROTOCOL_VERSION,
        "id": "hello-1",
        "method": " ",
        "params": {}
    }))
    .expect_err("empty method must be rejected");
    assert!(missing_method.to_string().contains("method"));
}

#[test]
fn malformed_or_oversized_frames_are_rejected() {
    let malformed = b"Content-Length: nope\r\n\r\n{}";
    let error = read_frame(&mut Cursor::new(malformed)).expect_err("invalid length");
    assert!(matches!(error, FrameError::InvalidHeader(_)));

    let truncated = b"Content-Length: 5\r\n\r\n{}";
    let error = read_frame(&mut Cursor::new(truncated)).expect_err("truncated body");
    assert!(matches!(error, FrameError::UnexpectedEof));
}

#[test]
fn duplicate_content_length_headers_are_rejected() {
    let duplicate = b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}";
    let error = read_frame(&mut Cursor::new(duplicate))
        .expect_err("duplicate content length must be rejected");
    assert!(matches!(error, FrameError::InvalidHeader(_)));
}

#[test]
fn request_round_trip_preserves_typed_fields() {
    let request = WorkerRequest::new("run-1", "cancel", json!({"task_id": "job-9"}));
    let decoded = parse_request(serde_json::to_value(&request).expect("serialize request"))
        .expect("parse request");
    assert_eq!(decoded.id, "run-1");
    assert_eq!(decoded.params["task_id"], "job-9");
}

#[test]
fn response_envelope_requires_the_current_protocol_version() {
    let bytes = encode_frame(&json!({
        "jsonrpc": "1.0",
        "protocol_version": "0.9",
        "id": "run-1",
        "result": {"state": "completed"}
    }))
    .expect("encode response");

    let error = bloomery::compute::worker::read_response(&mut Cursor::new(bytes), "run-1")
        .expect_err("response with stale protocol metadata must be rejected");
    assert!(error.to_string().contains("protocol"));
}

#[test]
fn write_frame_emits_exactly_one_complete_frame() {
    let mut output = Vec::new();
    write_frame(&mut output, &json!({"message": "温度"})).expect("write frame");
    let mut input = Cursor::new(output.clone());
    assert_eq!(
        read_frame(&mut input).expect("read frame"),
        Some(json!({"message": "温度"}))
    );
    assert_eq!(input.position() as usize, output.len());
    output.write_all(b"trailing").expect("append second stream");
    assert_eq!(
        read_frame(&mut Cursor::new(output)).unwrap().unwrap()["message"],
        "温度"
    );
}
