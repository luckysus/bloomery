use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};

pub const PROTOCOL_VERSION: &str = "1.0";
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    Io(String),
    InvalidHeader(String),
    InvalidJson(String),
    InvalidRequest(String),
    TooLarge { actual: usize, maximum: usize },
    UnexpectedEof,
}

impl Display for FrameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::InvalidHeader(message) => write!(formatter, "invalid frame header: {message}"),
            Self::InvalidJson(message) => write!(formatter, "invalid JSON frame: {message}"),
            Self::InvalidRequest(message) => write!(formatter, "invalid worker request: {message}"),
            Self::TooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "frame body is too large: {actual} bytes (maximum {maximum})"
                )
            }
            Self::UnexpectedEof => formatter.write_str("unexpected end of worker frame"),
        }
    }
}

impl std::error::Error for FrameError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkerRequest {
    pub jsonrpc: String,
    pub protocol_version: String,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl WorkerRequest {
    pub fn new(id: impl Into<String>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkerResponse {
    pub jsonrpc: String,
    pub protocol_version: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WorkerError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkerError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkerNotification {
    pub jsonrpc: String,
    pub protocol_version: String,
    pub method: String,
    pub params: Value,
}

pub fn encode_frame(value: &Value) -> Result<Vec<u8>, FrameError> {
    let body =
        serde_json::to_vec(value).map_err(|error| FrameError::InvalidJson(error.to_string()))?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            actual: body.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn write_frame<W: Write>(writer: &mut W, value: &Value) -> Result<(), FrameError> {
    let frame = encode_frame(value)?;
    writer
        .write_all(&frame)
        .map_err(|error| FrameError::Io(error.to_string()))?;
    writer
        .flush()
        .map_err(|error| FrameError::Io(error.to_string()))
}

pub fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Value>, FrameError> {
    let mut header = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        let count = reader
            .read(&mut byte)
            .map_err(|error| FrameError::Io(error.to_string()))?;
        if count == 0 {
            if header.is_empty() {
                return Ok(None);
            }
            return Err(FrameError::UnexpectedEof);
        }
        header.push(byte[0]);
        if header.len() > MAX_HEADER_BYTES {
            return Err(FrameError::InvalidHeader("header is too large".to_string()));
        }
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let header_text = std::str::from_utf8(&header)
        .map_err(|_| FrameError::InvalidHeader("header is not UTF-8".to_string()))?;
    let mut content_length = None;
    for line in header_text.split("\r\n") {
        let Some(value) = line.strip_prefix("Content-Length:") else {
            continue;
        };
        if content_length.is_some() {
            return Err(FrameError::InvalidHeader(
                "duplicate Content-Length".to_string(),
            ));
        }
        content_length = Some(value.trim().parse::<usize>().map_err(|_| {
            FrameError::InvalidHeader("Content-Length must be an integer".to_string())
        })?);
    }
    let content_length = content_length
        .ok_or_else(|| FrameError::InvalidHeader("Content-Length is required".to_string()))?;
    if content_length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            actual: content_length,
            maximum: MAX_FRAME_BYTES,
        });
    }

    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => FrameError::UnexpectedEof,
            _ => FrameError::Io(error.to_string()),
        })?;
    serde_json::from_slice(&body).map_err(|error| FrameError::InvalidJson(error.to_string()))
}

pub fn parse_request(value: Value) -> Result<WorkerRequest, FrameError> {
    let request: WorkerRequest = serde_json::from_value(value)
        .map_err(|error| FrameError::InvalidRequest(error.to_string()))?;
    if request.jsonrpc != "2.0" {
        return Err(FrameError::InvalidRequest(
            "jsonrpc must be 2.0".to_string(),
        ));
    }
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(FrameError::InvalidRequest(format!(
            "protocol_version must be {PROTOCOL_VERSION}"
        )));
    }
    if request.id.trim().is_empty() {
        return Err(FrameError::InvalidRequest(
            "id must not be empty".to_string(),
        ));
    }
    if request.method.trim().is_empty() {
        return Err(FrameError::InvalidRequest(
            "method must not be empty".to_string(),
        ));
    }
    Ok(request)
}
