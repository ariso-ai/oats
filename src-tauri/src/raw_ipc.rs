//! Helpers for commands that receive a large binary payload over the IPC bridge.
//!
//! Tauri only takes the raw-body fast path when the byte array *is* the entire
//! args payload. Measured in-app against a 48 MB recording (50 min @ 128 kbps),
//! sending the same bytes four ways:
//!
//! | payload shape                        | invoke |
//! |--------------------------------------|--------|
//! | raw `Uint8Array` as the whole args   |  44 ms |
//! | nested `{ audio: Uint8Array }`       |  15.0 s |
//! | nested `{ audio: ArrayBuffer }`      |  14.8 s |
//! | `number[]`                           |  29.7 s |
//!
//! So the audio travels as the body and every other argument rides along in a
//! header. Encoding the header as hex-of-JSON (rather than plain JSON) keeps it
//! within the visible-ASCII range HTTP header values require, which matters
//! because a recording title can contain arbitrary Unicode.

use tauri::ipc::{InvokeBody, Request};

/// Header carrying the hex-encoded JSON arguments of a raw-body command.
pub const META_HEADER: &str = "x-oats-meta";

/// Upper bound on a raw body, matching the `MAX_AUDIO_BYTES` bound the audio
/// *read* paths already enforce — a recording too large to read back is not
/// worth writing. ~17 hours of 128 kbps mp3. Commands are only reachable from
/// our own webviews, but the raw path is fast enough (48 MB in ~44 ms) that it
/// no longer throttles a runaway caller the way JSON serialization did.
pub const MAX_BODY_BYTES: usize = 1024 * 1024 * 1024;

/// The raw payload bytes. Rejects a JSON body so a frontend that regresses to
/// `number[]` fails loudly here instead of silently costing 30 seconds.
pub fn body_bytes(request: &Request<'_>) -> Result<Vec<u8>, String> {
    match request.body() {
        InvokeBody::Raw(bytes) if bytes.len() > MAX_BODY_BYTES => Err(format!(
            "payload of {} bytes exceeds the {MAX_BODY_BYTES}-byte limit",
            bytes.len()
        )),
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        InvokeBody::Json(_) => {
            Err("expected a raw binary body; got JSON (see raw_ipc)".to_string())
        }
    }
}

/// The command's non-binary arguments, decoded from [`META_HEADER`].
pub fn meta<T: serde::de::DeserializeOwned>(request: &Request<'_>) -> Result<T, String> {
    let header = request
        .headers()
        .get(META_HEADER)
        .ok_or_else(|| format!("missing {META_HEADER} header"))?
        .to_str()
        .map_err(|e| format!("non-ASCII {META_HEADER} header: {e}"))?;
    let json = hex::decode(header).map_err(|e| format!("malformed {META_HEADER} hex: {e}"))?;
    serde_json::from_slice(&json).map_err(|e| format!("malformed {META_HEADER} JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        title: String,
        duration_seconds: u64,
        append_to: Option<String>,
    }

    /// `Request` cannot be constructed outside Tauri, so the header contract is
    /// tested through the same decode chain `meta` uses.
    fn decode(header: &str) -> Result<Args, String> {
        let json = hex::decode(header).map_err(|e| format!("malformed {META_HEADER} hex: {e}"))?;
        serde_json::from_slice(&json).map_err(|e| format!("malformed {META_HEADER} JSON: {e}"))
    }

    fn encode(json: &str) -> String {
        hex::encode(json.as_bytes())
    }

    #[test]
    fn decodes_hex_encoded_json_args() {
        let header = encode(r#"{"title":"Mon Jun 2 @ 2PM","durationSeconds":2400}"#);
        assert_eq!(
            decode(&header).unwrap(),
            Args {
                title: "Mon Jun 2 @ 2PM".to_string(),
                duration_seconds: 2400,
                append_to: None,
            }
        );
    }

    /// The reason the header is hex rather than raw JSON: titles are user- and
    /// locale-derived, and a non-ASCII byte is not a legal header value.
    #[test]
    fn survives_non_ascii_titles() {
        let header = encode(r#"{"title":"打ち合わせ ☕","durationSeconds":60}"#);
        assert_eq!(decode(&header).unwrap().title, "打ち合わせ ☕");
    }

    #[test]
    fn optional_args_may_be_absent_or_null() {
        let header = encode(r#"{"title":"T","durationSeconds":1,"appendTo":null}"#);
        assert_eq!(decode(&header).unwrap().append_to, None);
        let header = encode(r#"{"title":"T","durationSeconds":1,"appendTo":"2026-06-02T10-00-00Z"}"#);
        assert_eq!(
            decode(&header).unwrap().append_to.as_deref(),
            Some("2026-06-02T10-00-00Z")
        );
    }

    #[test]
    fn rejects_malformed_headers() {
        assert!(decode("not hex").unwrap_err().contains("hex"));
        assert!(decode(&encode("{not json")).unwrap_err().contains("JSON"));
    }
}
