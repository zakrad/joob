use aes_gcm::{
    aead::{Aead, Nonce},
    Aes256Gcm, KeyInit,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Sha256;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

// ─── CryptoError ───────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("ciphertext too short to contain nonce")]
    CiphertextTooShort,

    #[error("decryption failed: invalid ciphertext or authentication tag")]
    DecryptionFailed,

    #[error("HKDF expansion failed")]
    HkdfError,
}

// ─── TunnelKey ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct TunnelKey([u8; 32]);

impl TunnelKey {
    /// Generate a cryptographically random 256-bit key.
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        Self(key)
    }

    /// Construct from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Serialize for TunnelKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let encoded = BASE64.encode(self.0);
        serializer.serialize_str(&encoded)
    }
}

impl<'de> Deserialize<'de> for TunnelKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = BASE64
            .decode(&s)
            .map_err(serde::de::Error::custom)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes for TunnelKey"))?;
        Ok(TunnelKey(arr))
    }
}

// ─── Cipher ────────────────────────────────────────────────────────────────────

/// AES-256-GCM cipher with an incrementing nonce counter.
///
/// Nonce layout (12 bytes):
///   [0..4]  — direction prefix (prevents nonce reuse between client/exit)
///   [4..12] — big-endian 64-bit counter
pub struct Cipher {
    cipher: Aes256Gcm,
    nonce_counter: AtomicU64,
    direction_prefix: [u8; 4],
}

impl Cipher {
    /// Create a new cipher for the given key and direction prefix.
    ///
    /// The `direction_prefix` (4 bytes) should differ between the client→exit
    /// and exit→client directions to guarantee nonce uniqueness.
    pub fn new(key: &TunnelKey, direction_prefix: [u8; 4]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(key.as_bytes()).expect("valid 32-byte key");
        Self {
            cipher,
            nonce_counter: AtomicU64::new(0),
            direction_prefix,
        }
    }

    /// Encrypt `plaintext` and return `nonce || ciphertext || tag`.
    pub fn seal(&self, plaintext: &[u8]) -> Vec<u8> {
        let counter = self.nonce_counter.fetch_add(1, Ordering::Relaxed);
        let nonce_bytes = self.build_nonce(counter);
        let nonce = Nonce::<Aes256Gcm>::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .expect("encryption should not fail with valid inputs");

        let mut sealed = Vec::with_capacity(12 + ciphertext.len());
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        sealed
    }

    /// Decrypt a sealed message (nonce || ciphertext || tag).
    pub fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if sealed.len() < 12 + 16 {
            // minimum: 12-byte nonce + 16-byte GCM tag (empty plaintext)
            return Err(CryptoError::CiphertextTooShort);
        }

        let (nonce_bytes, ciphertext) = sealed.split_at(12);
        let nonce = Nonce::<Aes256Gcm>::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::DecryptionFailed)
    }

    fn build_nonce(&self, counter: u64) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&self.direction_prefix);
        nonce[4..12].copy_from_slice(&counter.to_be_bytes());
        nonce
    }
}

// ─── SessionCipher ─────────────────────────────────────────────────────────────

/// Derives a session-specific cipher from a master `TunnelKey` and a `session_id`
/// using HKDF-SHA256.
pub struct SessionCipher;

impl SessionCipher {
    /// Derive a new `TunnelKey` from `master` and `session_id`.
    pub fn derive_key(master: &TunnelKey, session_id: &[u8]) -> Result<TunnelKey, CryptoError> {
        let hk = Hkdf::<Sha256>::new(Some(session_id), master.as_bytes());
        let mut okm = [0u8; 32];
        hk.expand(b"joob-tunnel-session-key", &mut okm)
            .map_err(|_| CryptoError::HkdfError)?;
        Ok(TunnelKey(okm))
    }

    /// Convenience: derive key and build a `Cipher` for the given direction.
    pub fn new(
        master: &TunnelKey,
        session_id: &[u8],
        direction_prefix: [u8; 4],
    ) -> Result<Cipher, CryptoError> {
        let key = Self::derive_key(master, session_id)?;
        Ok(Cipher::new(&key, direction_prefix))
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_encrypt_decrypt() {
        let key = TunnelKey::generate();
        let cipher = Cipher::new(&key, [0x00, 0x00, 0x00, 0x01]);

        let plaintext = b"hello, joob tunnel!";
        let sealed = cipher.seal(plaintext);
        let opened = cipher.open(&sealed).expect("decryption should succeed");

        assert_eq!(opened, plaintext);
    }

    #[test]
    fn tampered_ciphertext_detected() {
        let key = TunnelKey::generate();
        let cipher = Cipher::new(&key, [0x00, 0x00, 0x00, 0x01]);

        let plaintext = b"sensitive data";
        let mut sealed = cipher.seal(plaintext);

        // Flip a byte in the ciphertext portion (after the 12-byte nonce)
        let idx = 12 + (sealed.len() - 12) / 2;
        sealed[idx] ^= 0xFF;

        let result = cipher.open(&sealed);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CryptoError::DecryptionFailed));
    }

    #[test]
    fn different_nonces_per_seal_call() {
        let key = TunnelKey::generate();
        let cipher = Cipher::new(&key, [0x00, 0x00, 0x00, 0x01]);

        let sealed1 = cipher.seal(b"message one");
        let sealed2 = cipher.seal(b"message two");

        // Extract the 12-byte nonces
        let nonce1 = &sealed1[..12];
        let nonce2 = &sealed2[..12];

        assert_ne!(nonce1, nonce2, "nonces must differ between seal() calls");
    }

    #[test]
    fn session_key_derivation_different_sessions() {
        let master = TunnelKey::generate();

        let key_a = SessionCipher::derive_key(&master, b"session-alpha").unwrap();
        let key_b = SessionCipher::derive_key(&master, b"session-beta").unwrap();

        assert_ne!(
            key_a.as_bytes(),
            key_b.as_bytes(),
            "different session IDs must produce different keys"
        );
    }

    #[test]
    fn serde_roundtrip() {
        let key = TunnelKey::generate();
        let json = serde_json::to_string(&key).unwrap();
        let restored: TunnelKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key.as_bytes(), restored.as_bytes());
    }

    #[test]
    fn ciphertext_too_short_error() {
        let key = TunnelKey::generate();
        let cipher = Cipher::new(&key, [0x00, 0x00, 0x00, 0x01]);

        let result = cipher.open(&[0u8; 10]); // too short
        assert!(matches!(result.unwrap_err(), CryptoError::CiphertextTooShort));
    }

    #[test]
    fn session_cipher_end_to_end() {
        let master = TunnelKey::generate();
        let session_id = b"unique-session-42";
        let direction = [0x01, 0x02, 0x03, 0x04];

        let cipher = SessionCipher::new(&master, session_id, direction).unwrap();
        let plaintext = b"session-specific secret";
        let sealed = cipher.seal(plaintext);
        let opened = cipher.open(&sealed).unwrap();

        assert_eq!(opened, plaintext);
    }
}
