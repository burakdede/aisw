/// Shared JWT payload decoding utilities.
///
/// All JWT handling in aisw uses this module. Do NOT add ad-hoc base64 decoding
/// elsewhere — the `base64` crate is already a project dependency.
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;

/// Decode the payload segment of a JWT and return the parsed JSON value.
///
/// Accepts both padded (`URL_SAFE`) and unpadded (`URL_SAFE_NO_PAD`) base64url
/// encoding, which covers all real-world JWT implementations.
/// Returns `None` if the token has fewer than two `.`-separated segments, the
/// payload is not valid base64url, or the decoded bytes are not valid JSON.
pub(crate) fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload_b64 = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .or_else(|_| URL_SAFE.decode(payload_b64))
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Encode bytes as unpadded base64url — for constructing JWT test fixtures only.
#[cfg(test)]
pub(crate) fn encode_jwt_payload_for_test(input: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jwt(payload_json: &str) -> String {
        let header = "eyJhbGciOiJIUzI1NiJ9";
        let payload = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        format!("{header}.{payload}.sig")
    }

    fn make_jwt_padded(payload_json: &str) -> String {
        let header = "eyJhbGciOiJIUzI1NiJ9";
        let payload = URL_SAFE.encode(payload_json.as_bytes());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn decodes_unpadded_payload() {
        let jwt = make_jwt(r#"{"email":"user@example.com"}"#);
        let v = decode_jwt_payload(&jwt).unwrap();
        assert_eq!(v["email"], "user@example.com");
    }

    #[test]
    fn decodes_padded_payload() {
        let jwt = make_jwt_padded(r#"{"sub":"USER-1234"}"#);
        let v = decode_jwt_payload(&jwt).unwrap();
        assert_eq!(v["sub"], "USER-1234");
    }

    #[test]
    fn decodes_exp_claim() {
        let jwt = make_jwt(r#"{"exp":1893456000}"#);
        let v = decode_jwt_payload(&jwt).unwrap();
        assert_eq!(v["exp"].as_i64(), Some(1893456000));
    }

    #[test]
    fn returns_none_for_too_few_segments() {
        assert!(decode_jwt_payload("only_one_part").is_none());
        assert!(decode_jwt_payload("header.").is_none());
    }

    #[test]
    fn returns_none_for_invalid_base64() {
        assert!(decode_jwt_payload("header.bad$payload.sig").is_none());
    }

    #[test]
    fn returns_none_for_non_json_payload() {
        let header = "eyJhbGciOiJIUzI1NiJ9";
        let payload = URL_SAFE_NO_PAD.encode(b"not json at all");
        let jwt = format!("{header}.{payload}.sig");
        assert!(decode_jwt_payload(&jwt).is_none());
    }
}
