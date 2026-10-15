//! Socket handshake lines exchanged before a phone socket switches to raw V4
//! lines (REMOTE_CONTROL.md, "LAN slice" › Transport and pairing).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// How long a peer may take to send its next handshake line.
pub(crate) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const DEFAULT_PORT: u16 = 47821;

/// Phone → desktop.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum PhoneHello {
    #[serde(rename_all = "camelCase")]
    Pair {
        pairing_token: String,
        device_name: String,
    },
    #[serde(rename_all = "camelCase")]
    Auth { device_token: String },
    #[serde(rename_all = "camelCase")]
    Open { workspace_key: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct RemoteWorkspace {
    pub key: String,
    pub display: String,
    /// `project` or `conversation`.
    pub purpose: String,
}

/// Desktop → phone.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum DesktopReply {
    #[serde(rename_all = "camelCase")]
    Paired {
        device_token: String,
    },
    Workspaces {
        workspaces: Vec<RemoteWorkspace>,
    },
    Opened,
    Error {
        code: String,
    },
}

impl DesktopReply {
    pub(crate) fn line(&self) -> String {
        serde_json::to_string(self).expect("handshake reply serializes")
    }

    pub(crate) fn error(code: &str) -> String {
        Self::Error { code: code.into() }.line()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shapes_match_the_spec() {
        let pair: PhoneHello =
            serde_json::from_str(r#"{"type":"pair","pairingToken":"t","deviceName":"Pixel"}"#)
                .unwrap();
        assert_eq!(
            pair,
            PhoneHello::Pair {
                pairing_token: "t".into(),
                device_name: "Pixel".into()
            }
        );
        let open: PhoneHello =
            serde_json::from_str(r#"{"type":"open","workspaceKey":"C:\\w"}"#).unwrap();
        assert_eq!(
            open,
            PhoneHello::Open {
                workspace_key: "C:\\w".into()
            }
        );
        assert_eq!(
            DesktopReply::Paired {
                device_token: "d".into()
            }
            .line(),
            r#"{"type":"paired","deviceToken":"d"}"#
        );
        assert_eq!(DesktopReply::Opened.line(), r#"{"type":"opened"}"#);
        assert_eq!(DesktopReply::error("x"), r#"{"type":"error","code":"x"}"#);
    }
}
