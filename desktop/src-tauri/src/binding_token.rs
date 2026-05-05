// SPDX-License-Identifier: AGPL-3.0-only
//! Pseudonymous LMS binding tokens (`HMAC-SHA256` over a canonical preimage).
//! Encoding is part of the persisted desktop data contract.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::errors::HostResult;

type HmacSha256 = Hmac<Sha256>;

/// Must match persisted `token_version` column when storing bindings.
pub const TOKEN_VERSION: i32 = 1;

/// Stable course scope for Canvas-linked projects (ties token to one course).
pub fn canvas_course_context(course_id: &str) -> String {
    format!("canvas:course:{}", course_id.trim())
}

/// Canonical UTF-8 preimage: versioned label, then fields separated by U+001F (unit separator).
pub fn binding_preimage_bytes(
    course_context: &str,
    stable_lms_student_id: &str,
    token_version: i32,
) -> Vec<u8> {
    let mut s = String::new();
    s.push_str("scriptscore:lms_binding:v1");
    s.push('\x1f');
    s.push_str(course_context.trim());
    s.push('\x1f');
    s.push_str(stable_lms_student_id.trim());
    s.push('\x1f');
    s.push_str(&token_version.to_string());
    s.into_bytes()
}

/// Hex-encoded HMAC-SHA256 (64 hex chars).
pub fn compute_binding_token_hex(
    secret: &[u8],
    course_context: &str,
    stable_lms_student_id: &str,
    token_version: i32,
) -> HostResult<String> {
    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|_| crate::errors::HostError::Project("Invalid HMAC key length.".into()))?;
    mac.update(&binding_preimage_bytes(
        course_context,
        stable_lms_student_id,
        token_version,
    ));
    let out = mac.finalize().into_bytes();
    Ok(hex::encode(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preimage_is_deterministic() {
        let a = binding_preimage_bytes("canvas:course:1", "42", 1);
        let b = binding_preimage_bytes("canvas:course:1", "42", 1);
        assert_eq!(a, b);
    }

    #[test]
    fn token_hex_is_deterministic_for_fixed_secret() {
        let secret = [7u8; 32];
        let t1 = compute_binding_token_hex(&secret, "canvas:course:9", "1001", 1).unwrap();
        let t2 = compute_binding_token_hex(&secret, "canvas:course:9", "1001", 1).unwrap();
        assert_eq!(t1.len(), 64);
        assert_eq!(t1, t2);
    }

    #[test]
    fn different_student_yields_different_token() {
        let secret = [7u8; 32];
        let a = compute_binding_token_hex(&secret, "canvas:course:9", "1", 1).unwrap();
        let b = compute_binding_token_hex(&secret, "canvas:course:9", "2", 1).unwrap();
        assert_ne!(a, b);
    }
}
