// SPDX-License-Identifier: AGPL-3.0-only
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::errors::{HostError, HostResult};

pub const PROTOCOL_VERSION: &str = "desktop.sidecar.v1";

#[derive(Debug, Serialize)]
pub struct ProtocolRequest<P: Serialize> {
    #[serde(rename = "type")]
    pub request_type: &'static str,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub payload: P,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProtocolMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_id: Option<String>,
    pub job_id: Option<String>,
    pub payload: Value,
}

pub fn write_frame<W: Write>(writer: &mut W, payload: &impl Serialize) -> HostResult<()> {
    let message = serde_json::to_vec(payload)?;
    let length = u32::try_from(message.len())
        .map_err(|_| HostError::Protocol("Protocol payload exceeded 4-byte frame limit.".into()))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&message)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame<R: Read>(reader: &mut R) -> HostResult<Option<ProtocolMessage>> {
    let mut header = [0_u8; 4];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err.into()),
    }
    let payload_len = u32::from_be_bytes(header) as usize;
    let mut payload = vec![0_u8; payload_len];
    reader.read_exact(&mut payload)?;
    Ok(Some(serde_json::from_slice(&payload)?))
}

pub fn hello_request(request_id: String) -> ProtocolRequest<Map<String, Value>> {
    let mut payload = Map::new();
    payload.insert(
        "protocol_version".into(),
        Value::String(PROTOCOL_VERSION.to_string()),
    );
    ProtocolRequest {
        request_type: "hello",
        request_id,
        job_id: None,
        payload,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{hello_request, read_frame, write_frame};

    #[test]
    fn frame_round_trip_preserves_message() {
        let mut bytes = Vec::new();
        let request = hello_request("req_1".into());

        write_frame(&mut bytes, &request).expect("frame should encode");

        let decoded = read_frame(&mut bytes.as_slice())
            .expect("frame should decode")
            .expect("frame should exist");
        assert_eq!(decoded.message_type, "hello");
        assert_eq!(decoded.request_id.as_deref(), Some("req_1"));
        assert_eq!(
            decoded
                .payload
                .get("protocol_version")
                .and_then(Value::as_str),
            Some("desktop.sidecar.v1")
        );
    }
}
