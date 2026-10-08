//! Bounded implementation of the public RPC tags and little-endian 7-bit VQL.
use serde_json::Value;
use std::io::Write;

pub(super) const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_DEPTH: usize = 32;
pub(super) const MAX_COLLECTION_ITEMS: usize = 65_536;
const MAX_NODES: usize = 100_000;
const INVALID: &str = "Services RPC invalid or excessive serialization";

#[derive(Debug, Clone, PartialEq)]
pub(super) enum WireValue {
    Undefined,
    Json(Value),
    Buffer(Vec<u8>),
    VsBuffer(Vec<u8>),
    Array(Vec<WireValue>),
}

impl WireValue {
    pub(super) fn into_json(self) -> Result<Value, String> {
        match self {
            Self::Json(value) => Ok(value),
            Self::Array(values) => values
                .into_iter()
                .map(Self::into_json)
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            // undefined 或二进制不能冒充 JSON null，调用方必须明确处理返回类型。
            _ => Err("Services RPC non-JSON value in JSON result".into()),
        }
    }
}

struct Budget(usize);
impl Budget {
    fn visit(&mut self, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH || self.0 == MAX_NODES {
            return Err(INVALID.into());
        }
        self.0 += 1;
        Ok(())
    }
}

fn check_json(value: &Value, depth: usize, budget: &mut Budget) -> Result<(), String> {
    budget.visit(depth)?;
    match value {
        Value::String(text) if text.len() > MAX_TEXT_BYTES => return Err(INVALID.into()),
        Value::Array(values) => {
            if values.len() > MAX_COLLECTION_ITEMS {
                return Err(INVALID.into());
            }
            for value in values {
                check_json(value, depth + 1, budget)?;
            }
        }
        Value::Object(values) => {
            // TS reviver 将这个保留标记恢复为 Uint8Array；JSON-only API 不能把它冒充对象。
            if values.len() == 2
                && values.get("__zcode_rpc_nested_uint8array_v1") == Some(&Value::Bool(true))
                && values.get("base64").is_some_and(Value::is_string)
            {
                return Err("Services RPC nested binary is unsupported by JSON API".into());
            }
            if values.len() > MAX_COLLECTION_ITEMS {
                return Err(INVALID.into());
            }
            for (key, value) in values {
                if key.len() > MAX_TEXT_BYTES {
                    return Err(INVALID.into());
                }
                check_json(value, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct Output(Vec<u8>);
impl Output {
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.0.len().saturating_add(bytes.len()) > MAX_FRAME_BYTES {
            return Err(INVALID.into());
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
    fn vql(&mut self, mut value: u32) -> Result<(), String> {
        loop {
            let mut byte = (value & 127) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 128;
            }
            self.bytes(&[byte])?;
            if value == 0 {
                return Ok(());
            }
        }
    }
    fn blob(&mut self, tag: u8, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(INVALID.into());
        }
        self.bytes(&[tag])?;
        self.vql(bytes.len() as u32)?;
        self.bytes(bytes)
    }
}

struct JsonOutput(Vec<u8>);
impl Write for JsonOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_TEXT_BYTES {
            return Err(std::io::Error::other(INVALID));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn int32(value: &serde_json::Number) -> Option<i32> {
    let n = value.as_f64()?;
    (n >= i32::MIN as f64 && n <= i32::MAX as f64 && n.fract() == 0.0).then_some(n as i32)
}

fn write_json(
    out: &mut Output,
    value: &Value,
    depth: usize,
    budget: &mut Budget,
) -> Result<(), String> {
    match value {
        Value::Array(values) => {
            budget.visit(depth)?;
            if values.len() > MAX_COLLECTION_ITEMS {
                return Err(INVALID.into());
            }
            out.bytes(&[4])?;
            out.vql(values.len() as u32)?;
            for value in values {
                write_json(out, value, depth + 1, budget)?;
            }
            Ok(())
        }
        Value::String(text) => {
            budget.visit(depth)?;
            out.blob(1, text.as_bytes())
        }
        Value::Number(n) if int32(n).is_some() => {
            budget.visit(depth)?;
            out.bytes(&[6])?;
            // TS writeInt32VQL 用 >>>，负整数先按无符号 32 位写入，读取恢复有符号值。
            out.vql(int32(n).unwrap() as u32)
        }
        _ => {
            check_json(value, depth, budget)?;
            let mut json = JsonOutput(Vec::new());
            serde_json::to_writer(&mut json, value).map_err(|_| INVALID.to_string())?;
            out.blob(5, &json.0)
        }
    }
}

fn write_wire(
    out: &mut Output,
    value: &WireValue,
    depth: usize,
    budget: &mut Budget,
) -> Result<(), String> {
    match value {
        WireValue::Json(value) => write_json(out, value, depth, budget),
        WireValue::Undefined => {
            budget.visit(depth)?;
            out.bytes(&[0])
        }
        WireValue::Buffer(bytes) | WireValue::VsBuffer(bytes) => {
            budget.visit(depth)?;
            out.blob(
                if matches!(value, WireValue::Buffer(_)) {
                    2
                } else {
                    3
                },
                bytes,
            )
        }
        WireValue::Array(values) => {
            budget.visit(depth)?;
            if values.len() > MAX_COLLECTION_ITEMS {
                return Err(INVALID.into());
            }
            out.bytes(&[4])?;
            out.vql(values.len() as u32)?;
            for value in values {
                write_wire(out, value, depth + 1, budget)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
pub(super) fn encode_value(value: &WireValue) -> Result<Vec<u8>, String> {
    let mut out = Output(Vec::new());
    write_wire(&mut out, value, 0, &mut Budget(0))?;
    Ok(out.0)
}

pub(super) fn encode_pair(header: &WireValue, body: &WireValue) -> Result<Vec<u8>, String> {
    let (mut out, mut budget) = (Output(Vec::new()), Budget(0));
    write_wire(&mut out, header, 0, &mut budget)?;
    write_wire(&mut out, body, 0, &mut budget)?;
    Ok(out.0)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
    budget: Budget,
}
impl<'a> Decoder<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(length).ok_or(INVALID)?;
        let bytes = self.bytes.get(self.pos..end).ok_or(INVALID)?;
        self.pos = end;
        Ok(bytes)
    }
    fn vql(&mut self) -> Result<u32, String> {
        let mut value = 0;
        for shift in (0..=28).step_by(7) {
            let byte = self.take(1)?[0];
            if shift == 28 && byte > 15 {
                return Err(INVALID.into());
            }
            value |= ((byte & 127) as u32) << shift;
            if byte & 128 == 0 {
                return Ok(value);
            }
        }
        Err(INVALID.into())
    }
    fn blob(&mut self) -> Result<&'a [u8], String> {
        let length = self.vql()? as usize;
        if length > MAX_TEXT_BYTES {
            return Err(INVALID.into());
        }
        self.take(length)
    }
    fn value(&mut self, depth: usize) -> Result<WireValue, String> {
        let tag = self.take(1)?[0];
        // JSON fallback 的整棵树单独计数；其他标签在这里计数。
        if tag != 5 {
            self.budget.visit(depth)?;
        }
        match tag {
            0 => Ok(WireValue::Undefined),
            1 => Ok(WireValue::Json(Value::String(
                std::str::from_utf8(self.blob()?)
                    .map_err(|_| INVALID)?
                    .to_owned(),
            ))),
            2 => Ok(WireValue::Buffer(self.blob()?.to_vec())),
            3 => Ok(WireValue::VsBuffer(self.blob()?.to_vec())),
            4 => {
                let length = self.vql()? as usize;
                if length > MAX_COLLECTION_ITEMS || length > self.bytes.len() - self.pos {
                    return Err(INVALID.into());
                }
                let mut values = Vec::new();
                for _ in 0..length {
                    values.push(self.value(depth + 1)?);
                }
                Ok(WireValue::Array(values))
            }
            5 => {
                let bytes = self.blob()?;
                check_json_depth(bytes, depth)?;
                let value: Value = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
                check_json(&value, depth, &mut self.budget)?;
                Ok(WireValue::Json(value))
            }
            6 => Ok(WireValue::Json(Value::from(self.vql()? as i32))),
            _ => Err(INVALID.into()),
        }
    }
}

// 在 serde 分配 JSON 树之前拒绝深层输入；字符串里的括号与转义不计入嵌套。
fn check_json_depth(bytes: &[u8], initial: usize) -> Result<(), String> {
    let (mut depth, mut quoted, mut escaped) = (initial, false, false);
    for &byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_DEPTH + 1 {
                        return Err(INVALID.into());
                    }
                }
                b']' | b'}' => depth = depth.checked_sub(1).ok_or(INVALID)?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn decoder(bytes: &[u8]) -> Result<Decoder<'_>, String> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(INVALID.into());
    }
    Ok(Decoder {
        bytes,
        pos: 0,
        budget: Budget(0),
    })
}

#[cfg(test)]
pub(super) fn decode_value(bytes: &[u8]) -> Result<WireValue, String> {
    let mut decoder = decoder(bytes)?;
    let value = decoder.value(0)?;
    if decoder.pos != bytes.len() {
        return Err(INVALID.into());
    }
    Ok(value)
}

pub(super) fn decode_pair(bytes: &[u8]) -> Result<(WireValue, WireValue), String> {
    let mut decoder = decoder(bytes)?;
    let pair = (decoder.value(0)?, decoder.value(0)?);
    if decoder.pos != bytes.len() {
        return Err(INVALID.into());
    }
    Ok(pair)
}
