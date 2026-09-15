//! The connection token and the HMAC handshake (docs/prd.md §6.2).
//!
//! At launch the app writes a fresh random token to `%APPDATA%\Dex\token`,
//! which only this user can read. Both ends of every connection prove they
//! know it by returning HMAC-SHA256(token, the other side's nonce). A client
//! that reached an impostor pipe finds out before sending anything, and an
//! impostor client never gets past the handshake.
//!
//! The CLI carries its own copy of `sign`/`verify` (it must stay free of the
//! daemon's dependencies); both copies are checked against the same RFC 4231
//! test vector, so they cannot drift apart silently.

use std::path::Path;
use std::{fs, io};

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const TOKEN_BYTES: usize = 32;
const NONCE_BYTES: usize = 16;

/// The shared secret between the app and its clients.
#[derive(Clone)]
pub struct Token {
    bytes: [u8; TOKEN_BYTES],
}

impl std::fmt::Debug for Token {
    /// Never prints the secret, so it cannot end up in a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

impl Token {
    /// A fresh random token.
    pub fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).map_err(|err| io::Error::other(err.to_string()))?;
        Ok(Self { bytes })
    }

    /// Writes the token as hex, replacing any previous token.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        fs::write(path, to_hex(&self.bytes))
    }

    /// Reads a token written by `write`.
    pub fn read(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        let bytes = from_hex(text.trim())
            .filter(|bytes| bytes.len() == TOKEN_BYTES)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "token file is corrupt"))?;
        let mut token = [0u8; TOKEN_BYTES];
        token.copy_from_slice(&bytes);
        Ok(Self { bytes: token })
    }
}

/// A fresh random nonce, hex-encoded.
pub fn nonce() -> io::Result<String> {
    let mut bytes = [0u8; NONCE_BYTES];
    getrandom::fill(&mut bytes).map_err(|err| io::Error::other(err.to_string()))?;
    Ok(to_hex(&bytes))
}

/// HMAC-SHA256(token, message), hex-encoded.
pub fn sign(token: &Token, message: &str) -> String {
    mac_hex(&token.bytes, message)
}

/// Whether `proof` is HMAC-SHA256(token, message). Compares in constant time.
pub fn verify(token: &Token, message: &str, proof: &str) -> bool {
    let Some(expected) = from_hex(proof) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(&token.bytes) else {
        return false;
    };
    mac.update(message.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

fn mac_hex(key: &[u8], message: &str) -> String {
    // HMAC accepts keys of any length, so this cannot fail; an empty proof
    // would simply fail verification on the other side.
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        return String::new();
    };
    mac.update(message.as_bytes());
    to_hex(&mac.finalize().into_bytes())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231_test_case_1() {
        // The CLI's copy is checked against the same vector.
        assert_eq!(
            mac_hex(&[0x0b; 20], "Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn a_signature_verifies_only_with_the_same_token_and_message() {
        let token = Token::generate().unwrap();
        let other = Token::generate().unwrap();
        let proof = sign(&token, "nonce-1");
        assert!(verify(&token, "nonce-1", &proof));
        assert!(!verify(&token, "nonce-2", &proof));
        assert!(!verify(&other, "nonce-1", &proof));
        assert!(!verify(&token, "nonce-1", "not hex"));
    }

    #[test]
    fn tokens_round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        let token = Token::generate().unwrap();
        token.write(&path).unwrap();
        let read = Token::read(&path).unwrap();
        assert!(verify(&read, "x", &sign(&token, "x")));
    }

    #[test]
    fn nonces_are_fresh() {
        assert_ne!(nonce().unwrap(), nonce().unwrap());
    }
}
