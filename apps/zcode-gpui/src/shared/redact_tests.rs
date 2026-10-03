//! Tests for `shared/redact.rs`. Every fixture secret below is fake.

use super::*;

fn assert_scrubbed(input: &str, secret: &str) {
    let out = scrub(input);
    assert!(!out.contains(secret), "leaked {secret:?} in {out:?}");
    assert!(out.contains(REDACTED), "{out:?}");
}

#[test]
fn json_and_kv_credentials() {
    assert_scrubbed(
        r#"{"apiKey":"abc123secretvalue","model":"glm"}"#,
        "abc123secretvalue",
    );
    assert_scrubbed(r#"{"api_key": "k-1234"}"#, "k-1234");
    assert_scrubbed("ZAI_TOKEN token=deadbeefcafe&x=1", "deadbeefcafe");
    assert_scrubbed("password: hunter2 next", "hunter2");
    assert_scrubbed(r#"{\"refresh_token\":\"rt-998877\"}"#, "rt-998877");
    assert_scrubbed("client_secret='s3cr3t'", "s3cr3t");
    let out = scrub(r#"{"apiKey":"abc123secretvalue","model":"glm"}"#);
    assert!(
        out.contains(r#""model":"glm""#),
        "non-secret fields survive: {out}"
    );
}

#[test]
fn authorization_headers() {
    assert_scrubbed("Authorization: Bearer abcdefgh12345678", "abcdefgh12345678");
    assert_scrubbed("authorization=Basic dXNlcjpwYXNz", "dXNlcjpwYXNz");
    assert_scrubbed("curl -H 'X-Api-Key: zzzzyyyyxxxx'", "zzzzyyyyxxxx");
    assert_scrubbed("sending header bearer QWERTYUIOP123", "QWERTYUIOP123");
}

#[test]
fn self_identifying_token_shapes() {
    assert_scrubbed(
        "Incorrect API key provided: sk-proj-ABCDEFGHIJKLMNOPQRSTUV",
        "ABCDEFGHIJKLMNOPQRSTUV",
    );
    assert_scrubbed(
        "url https://h/?k=sk-ABCDEFGHIJKLMNOPQRST",
        "ABCDEFGHIJKLMNOPQRST",
    );
    assert_scrubbed("gh ghp_0123456789abcdef0123456789", "ghp_0123456789abcdef");
    assert_scrubbed("aws AKIAABCDEFGHIJKLMNOP done", "AKIAABCDEFGHIJKLMNOP");
    assert_scrubbed(
        "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl",
        "eyJzdWIiOiIxIn0",
    );
}

#[test]
fn ordinary_text_is_untouched() {
    for s in [
        "agent for C:/Users/me/proj exited; restarting in 1000ms",
        "maxTokens: 4096, inputTokens=12",
        "using sk-learn and hf_model for tokens",
        "history page: +40 rows",
        "Token budget exceeded",
        "",
        "非 ASCII 文本 password 说明",
    ] {
        assert_eq!(scrub(s), s);
    }
}

#[test]
fn non_ascii_around_secrets_keeps_char_boundaries() {
    let out = scrub("密钥 apiKey=秘密值 后");
    assert!(!out.contains("秘密值"));
    assert!(out.starts_with("密钥 apiKey="));
}
