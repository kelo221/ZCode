use super::codec::{WireValue, decode_pair, encode_pair};
use super::frame::wrap_frame;
use serde_json::{Value, json};

/// Successful void results are distinct from explicit JSON null.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceValue {
    Undefined,
    Json(Value),
}
impl ServiceValue {
    pub fn into_json(self) -> Result<Value, String> {
        match self {
            Self::Json(value) => Ok(value),
            Self::Undefined => Err("Services RPC expected JSON but returned undefined".into()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum Message {
    Initialize,
    Success(u32, ServiceValue),
    Failure(u32, String),
}

pub(super) fn request_frame(
    id: u32,
    channel: &str,
    method: &str,
    args: Vec<Value>,
) -> Result<Vec<u8>, String> {
    wrap_frame(&encode_pair(
        &WireValue::Json(json!([100, id, channel, method])),
        &WireValue::Json(Value::Array(args)),
    )?)
}

pub(super) fn cancel_frame(id: u32) -> Result<Vec<u8>, String> {
    wrap_frame(&encode_message(&[101, id], WireValue::Undefined)?)
}

pub(super) fn encode_message(header: &[u32], body: WireValue) -> Result<Vec<u8>, String> {
    encode_pair(&WireValue::Json(json!(header)), &body)
}

pub(super) fn decode_message(bytes: &[u8]) -> Result<Message, String> {
    let (header, body) = decode_pair(bytes)?;
    let header = header.into_json()?;
    let header = header
        .as_array()
        .ok_or("Services RPC invalid response header")?;
    let kind = header
        .first()
        .and_then(Value::as_u64)
        .ok_or("Services RPC invalid response type")?;
    if kind == 200 {
        if header.len() != 1 || body != WireValue::Undefined {
            return Err("Services RPC invalid Initialize".into());
        }
        return Ok(Message::Initialize);
    }
    if header.len() != 2 {
        return Err("Services RPC invalid response header".into());
    }
    // TS Int 反序列化为有符号32位；ID 达上限时终止连接而不复用旧 ID。
    let id = header[1]
        .as_u64()
        .filter(|id| *id <= i32::MAX as u64)
        .ok_or("Services RPC invalid response id")? as u32;
    match kind {
        201 => Ok(Message::Success(
            id,
            match body {
                WireValue::Undefined => ServiceValue::Undefined,
                value => ServiceValue::Json(value.into_json()?),
            },
        )),
        // 远端 message/stack/detail 可能包含凭据、路径或用户内容；不把 payload 拼入错误。
        202 => Ok(Message::Failure(
            id,
            "Services RPC remote method error (payload redacted)".into(),
        )),
        203 => Ok(Message::Failure(
            id,
            "Services RPC remote rejection (payload redacted)".into(),
        )),
        _ => Err("Services RPC unsupported response type".into()),
    }
}
