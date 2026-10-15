//! Desktop pairing state: one-time pairing tokens, the single paired phone
//! (stored as the SHA-256 of its device token only) and the per-IP limit on
//! failed attempts. Shared between listener threads and the Settings page
//! behind one mutex; `AppState`'s gateway owns it.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub(crate) const PAIRING_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const MAX_FAILURES: usize = 5;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairedDevice {
    pub name: String,
    pub token_sha256: String,
    pub paired_at_ms: u64,
}

/// `phone-remote.json` in the data dir.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingFile {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub device: Option<PairedDevice>,
}

pub(crate) struct PairingStore {
    path: Option<PathBuf>,
    pub(crate) file: PairingFile,
    pending: Vec<(String, Instant)>,
    failures: HashMap<IpAddr, Vec<Instant>>,
    /// Bumped whenever the paired device changes, so sockets authenticated
    /// for an older device can be told apart.
    pub(crate) device_generation: u64,
}

pub(crate) fn random_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .expect("system random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Compare secrets without an early exit on the first differing byte.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

impl PairingStore {
    pub(crate) fn path() -> PathBuf {
        crate::shared::data_paths::paths()
            .data_root()
            .join("phone-remote.json")
    }

    /// Load from `path`; a missing or unreadable file means "off, unpaired".
    pub(crate) fn load(path: Option<PathBuf>) -> Self {
        let file = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            file,
            pending: Vec::new(),
            failures: HashMap::new(),
            device_generation: 0,
        }
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.file).map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.file.enabled = enabled;
        self.save()
    }

    /// A fresh single-use pairing token, valid for `PAIRING_TOKEN_TTL`.
    pub(crate) fn issue_pairing_token(&mut self, now: Instant) -> String {
        self.pending
            .retain(|(_, issued)| now.duration_since(*issued) < PAIRING_TOKEN_TTL);
        let token = random_token();
        self.pending.push((token.clone(), now));
        token
    }

    /// Consume a pairing token. On success the phone replaces any previously
    /// paired device, and its new device token is returned (only once).
    pub(crate) fn pair(&mut self, token: &str, device_name: &str, now: Instant) -> Option<String> {
        let index = self.pending.iter().position(|(pending, issued)| {
            now.duration_since(*issued) < PAIRING_TOKEN_TTL
                && constant_time_eq(pending.as_bytes(), token.as_bytes())
        })?;
        self.pending.remove(index);
        let device_token = random_token();
        let name: String = device_name.chars().take(64).collect();
        self.file.device = Some(PairedDevice {
            name: if name.trim().is_empty() {
                "Phone".into()
            } else {
                name
            },
            token_sha256: sha256_hex(&device_token),
            paired_at_ms: now_ms(),
        });
        self.device_generation += 1;
        if let Err(error) = self.save() {
            eprintln!("[zcode-gpui] could not save phone pairing: {error}");
        }
        Some(device_token)
    }

    pub(crate) fn verify(&self, device_token: &str) -> bool {
        self.file.device.as_ref().is_some_and(|device| {
            constant_time_eq(
                device.token_sha256.as_bytes(),
                sha256_hex(device_token).as_bytes(),
            )
        })
    }

    pub(crate) fn revoke(&mut self) -> Result<(), String> {
        self.file.device = None;
        self.pending.clear();
        self.device_generation += 1;
        self.save()
    }

    /// False while `ip` has used up its failed attempts for the window.
    pub(crate) fn allow_attempt(&mut self, ip: IpAddr, now: Instant) -> bool {
        let entries = self.failures.entry(ip).or_default();
        entries.retain(|at| now.duration_since(*at) < FAILURE_WINDOW);
        entries.len() < MAX_FAILURES
    }

    pub(crate) fn record_failure(&mut self, ip: IpAddr, now: Instant) {
        self.failures.entry(ip).or_default().push(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> PairingStore {
        PairingStore::load(None)
    }

    #[test]
    fn pairing_token_is_single_use_and_replaces_the_device() {
        let mut s = store();
        let now = Instant::now();
        let token = s.issue_pairing_token(now);
        let first = s.pair(&token, "Pixel", now).expect("valid token pairs");
        assert!(s.verify(&first));
        assert_eq!(s.pair(&token, "Pixel", now), None, "token is single use");

        let token = s.issue_pairing_token(now);
        let second = s.pair(&token, "Other", now).unwrap();
        assert!(!s.verify(&first), "a new pairing replaces the old phone");
        assert!(s.verify(&second));
        assert_eq!(s.file.device.as_ref().unwrap().name, "Other");
        assert_ne!(s.file.device.as_ref().unwrap().token_sha256, second);
    }

    #[test]
    fn pairing_tokens_expire() {
        let mut s = store();
        let issued = Instant::now();
        let token = s.issue_pairing_token(issued);
        let later = issued + PAIRING_TOKEN_TTL + Duration::from_secs(1);
        assert_eq!(s.pair(&token, "Pixel", later), None);
    }

    #[test]
    fn revoke_forgets_the_device() {
        let mut s = store();
        let now = Instant::now();
        let token = s.issue_pairing_token(now);
        let device = s.pair(&token, "Pixel", now).unwrap();
        let generation = s.device_generation;
        s.revoke().unwrap();
        assert!(!s.verify(&device));
        assert!(s.device_generation > generation);
    }

    #[test]
    fn failed_attempts_are_rate_limited_per_ip() {
        let mut s = store();
        let ip: IpAddr = "192.168.1.9".parse().unwrap();
        let other: IpAddr = "192.168.1.10".parse().unwrap();
        let now = Instant::now();
        for _ in 0..MAX_FAILURES {
            assert!(s.allow_attempt(ip, now));
            s.record_failure(ip, now);
        }
        assert!(!s.allow_attempt(ip, now));
        assert!(s.allow_attempt(other, now));
        assert!(s.allow_attempt(ip, now + FAILURE_WINDOW));
    }

    #[test]
    fn file_round_trips_and_defaults_to_off() {
        let dir = std::env::temp_dir().join(format!("zcode-pairing-{}", uuid::Uuid::now_v7()));
        let path = dir.join("phone-remote.json");
        let mut s = PairingStore::load(Some(path.clone()));
        assert_eq!(s.file, PairingFile::default());
        s.set_enabled(true).unwrap();
        let token = s.issue_pairing_token(Instant::now());
        s.pair(&token, "Pixel", Instant::now()).unwrap();
        let loaded = PairingStore::load(Some(path));
        assert!(loaded.file.enabled);
        assert_eq!(loaded.file.device, s.file.device);
        let _ = std::fs::remove_dir_all(dir);
    }
}
