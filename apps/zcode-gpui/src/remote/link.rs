//! `zcode-gpui://pair?host=…&port=…&fp=…&t=…`: what the desktop's QR code
//! encodes and the phone opens. The fingerprint is the lowercase hex SHA-256
//! of the desktop's TLS certificate.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PairLink {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub token: String,
}

impl PairLink {
    pub(crate) fn to_url(&self) -> String {
        let mut url = url::Url::parse("zcode-gpui://pair").expect("static url");
        url.query_pairs_mut()
            .append_pair("host", &self.host)
            .append_pair("port", &self.port.to_string())
            .append_pair("fp", &self.fingerprint)
            .append_pair("t", &self.token);
        url.into()
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        let url = url::Url::parse(text.trim()).ok()?;
        if url.scheme() != "zcode-gpui" || url.host_str() != Some("pair") {
            return None;
        }
        let get = |key: &str| {
            url.query_pairs()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
                .filter(|v| !v.is_empty())
        };
        let fingerprint = get("fp")?.to_ascii_lowercase();
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            host: get("host")?,
            port: get("port")?.parse().ok()?,
            fingerprint,
            token: get("t")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairLink {
        PairLink {
            host: "192.168.1.20".into(),
            port: 47821,
            fingerprint: "ab".repeat(32),
            token: "tok_-en".into(),
        }
    }

    #[test]
    fn round_trips() {
        let link = sample();
        let url = link.to_url();
        assert!(url.starts_with("zcode-gpui://pair?host=192.168.1.20&port=47821&fp="));
        assert_eq!(PairLink::parse(&url), Some(link));
    }

    #[test]
    fn rejects_other_links_and_bad_fingerprints() {
        assert_eq!(PairLink::parse("https://pair?host=a&port=1&fp=x&t=y"), None);
        let mut link = sample();
        link.fingerprint = "zz".repeat(32);
        assert_eq!(PairLink::parse(&link.to_url()), None);
        let url = sample().to_url().replace("port=47821", "port=nope");
        assert_eq!(PairLink::parse(&url), None);
    }
}
