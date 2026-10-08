//! SocketProtocol framing, explicitly not PersistentProtocol ACK/replay.
use super::codec::MAX_FRAME_BYTES;
use std::io::{ErrorKind, Read};

pub(super) struct Frame {
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub(super) fn wrap_frame(payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err("Services RPC frame limit exceeded".into());
    }
    let mut bytes = Vec::with_capacity(13 + payload.len());
    bytes.extend([1, 0, 0, 0, 0, 0, 0, 0, 0]);
    bytes.extend((payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

pub(super) fn read_frame(reader: &mut impl Read) -> Result<Option<Frame>, String> {
    let mut header = [0; 13];
    // read_exact 区分帧边界 EOF 与截断；不因分片丢弃已经读取的 header。
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err("Services RPC stdout read failed".into()),
        }
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|_| "Services RPC truncated frame header")?;
    let length = u32::from_be_bytes(header[9..13].try_into().unwrap()) as usize;
    if length > MAX_FRAME_BYTES {
        return Err("Services RPC frame limit exceeded".into());
    }
    let mut payload = vec![0; length];
    reader
        .read_exact(&mut payload)
        .map_err(|_| "Services RPC truncated frame body")?;
    Ok(Some(Frame {
        kind: header[0],
        payload,
    }))
}
