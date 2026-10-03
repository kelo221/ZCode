//! Secret scrubbing for every text sink this app controls: the in-memory log,
//! stderr echo, error banners, backend `lastError` text, the panic log and
//! the exported diagnostic bundle.
//!
//! Three passes, all allocation-light and regex-free:
//! 1. `key = value` / `"key": "value"` pairs whose key names a credential;
//! 2. `Bearer <token>` / `Basic <token>` auth schemes anywhere in the text;
//! 3. self-identifying token shapes (`sk-…`, `ghp_…`, `AKIA…`, JWTs, …).
//!
//! False positives only cost log fidelity; a missed secret leaks a credential,
//! so the rules lean towards redacting.

pub const REDACTED: &str = "[REDACTED]";

/// Credential key names, lowercase. Matched at a word boundary and only when
/// followed by `:` or `=` (optionally through a closing quote).
const SECRET_KEYS: &[&str] = &[
    "apikey",
    "api_key",
    "api-key",
    "x-api-key",
    "authorization",
    "proxy-authorization",
    "access_token",
    "accesstoken",
    "refresh_token",
    "refreshtoken",
    "id_token",
    "idtoken",
    "session_token",
    "sessiontoken",
    "client_secret",
    "clientsecret",
    "private_key",
    "privatekey",
    "secret",
    "password",
    "passwd",
    "token",
    "cookie",
    "set-cookie",
];

/// Prefixes of self-identifying credentials (provider keys, PATs, cloud keys).
const TOKEN_PREFIXES: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xapp-",
    "hf_",
    "AKIA",
    "ASIA",
    "AIza",
];

/// Minimum characters after a prefix before a run counts as a token, so
/// words such as `sk-learn` or `hf_model` survive.
const TOKEN_MIN_TAIL: usize = 16;

/// Scrub credentials from `input`.
pub fn scrub(input: &str) -> String {
    let s = scrub_key_values(input);
    let s = scrub_auth_schemes(&s);
    scrub_token_shapes(&s)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

fn is_value_end(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b',' | b'&' | b';' | b'}' | b')' | b']' | b'"' | b'\'')
}

fn is_token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'~' | b'+' | b'/' | b'=')
}

/// Characters of a self-identifying token (base64url-ish; no `/`, `=`, `+`,
/// so `?key=sk-…` or `path/sk-…` still starts a run at the prefix).
fn is_shape_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')
}

fn skip_ws(b: &[u8], mut k: usize) -> usize {
    while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
        k += 1;
    }
    k
}

/// Longest credential key starting at `i` on a word boundary.
fn match_key_at(lower: &[u8], i: usize) -> Option<usize> {
    if i > 0 && is_word(lower[i - 1]) {
        return None;
    }
    SECRET_KEYS
        .iter()
        .filter(|k| lower[i..].starts_with(k.as_bytes()))
        .map(|k| k.len())
        .filter(|&n| lower.get(i + n).is_none_or(|&c| !is_word(c)))
        .max()
}

/// Byte span of the value that follows a key ending at `j`.
fn value_span(b: &[u8], j: usize) -> Option<(usize, usize)> {
    let len = b.len();
    let mut k = j;
    // Closing quote of the key, possibly JSON-escaped (`\"`).
    if k < len && b[k] == b'\\' {
        k += 1;
    }
    if k < len && (b[k] == b'"' || b[k] == b'\'') {
        k += 1;
    }
    k = skip_ws(b, k);
    if k >= len || !(b[k] == b':' || b[k] == b'=') {
        return None;
    }
    k = skip_ws(b, k + 1);
    let escaped = k < len && b[k] == b'\\';
    if escaped {
        k += 1;
    }
    if k >= len || matches!(b[k], b'{' | b'[') {
        return None;
    }
    if b[k] == b'"' || b[k] == b'\'' {
        let quote = b[k];
        let start = k + 1;
        let mut e = start;
        while e < len && b[e] != quote {
            if b[e] == b'\\' {
                if escaped {
                    break;
                }
                e += 1;
            }
            e += 1;
        }
        let e = e.min(len);
        return (e > start).then_some((start, e));
    }
    let start = k;
    let mut e = k;
    while e < len && !is_value_end(b[e]) {
        e += 1;
    }
    // `Authorization: Bearer <token>`: the scheme word is not the secret.
    let word = &b[start..e];
    if (word.eq_ignore_ascii_case(b"bearer") || word.eq_ignore_ascii_case(b"basic"))
        && e < len
        && b[e] == b' '
    {
        e += 1;
        while e < len && !is_value_end(b[e]) {
            e += 1;
        }
    }
    (e > start).then_some((start, e))
}

fn scrub_key_values(input: &str) -> String {
    let b = input.as_bytes();
    let lower = input.to_ascii_lowercase();
    let lb = lower.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut i) = (0, 0);
    while i < b.len() {
        if let Some(n) = match_key_at(lb, i)
            && let Some((vs, ve)) = value_span(b, i + n)
        {
            // Spans start and end at ASCII bytes, so they are char boundaries.
            out.push_str(&input[copied..vs]);
            out.push_str(REDACTED);
            copied = ve;
            i = ve;
            continue;
        }
        i += 1;
    }
    out.push_str(&input[copied..]);
    out
}

fn scrub_auth_schemes(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut i) = (0, 0);
    while i < b.len() {
        let boundary = i == 0 || !is_word(b[i - 1]);
        let scheme_len = [b"bearer ".as_slice(), b"basic ".as_slice()]
            .iter()
            .find(|s| b.len() >= i + s.len() && b[i..i + s.len()].eq_ignore_ascii_case(s))
            .map(|s| s.len());
        if boundary && let Some(n) = scheme_len {
            let start = i + n;
            let mut e = start;
            while e < b.len() && is_token_char(b[e]) {
                e += 1;
            }
            if e - start >= 8 && &input[start..e] != REDACTED {
                out.push_str(&input[copied..start]);
                out.push_str(REDACTED);
                copied = e;
                i = e;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&input[copied..]);
    out
}

fn looks_like_token(run: &str) -> bool {
    let prefixed = TOKEN_PREFIXES
        .iter()
        .any(|p| run.starts_with(p) && run.len() >= p.len() + TOKEN_MIN_TAIL);
    let jwt = run.starts_with("eyJ") && run.len() >= 30 && run.matches('.').count() >= 2;
    prefixed || jwt
}

fn scrub_token_shapes(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let (mut copied, mut i) = (0, 0);
    while i < b.len() {
        if !is_shape_char(b[i]) || (i > 0 && is_shape_char(b[i - 1])) {
            i += 1;
            continue;
        }
        let mut e = i;
        while e < b.len() && is_shape_char(b[e]) {
            e += 1;
        }
        if looks_like_token(&input[i..e]) {
            out.push_str(&input[copied..i]);
            out.push_str(REDACTED);
            copied = e;
        }
        i = e;
    }
    out.push_str(&input[copied..]);
    out
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
