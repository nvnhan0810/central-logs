//! AES-256-GCM sealing of refresh tokens. The sealed value is bound to its session id (AAD), so a
//! ciphertext copied to another session fails to decrypt.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rand::RngCore;
use rand::rngs::OsRng;

use crate::application::error::AppError;
use crate::application::ports::SecretSealer;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

pub struct AesSecretSealer {
    cipher: Aes256Gcm,
}

impl AesSecretSealer {
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self {
            cipher: Aes256Gcm::new(&key.into()),
        }
    }

    /// Parses a base64-encoded 32-byte key, as read from `CENTRAL_LOGS_SESSION_KEY`.
    pub fn from_base64(raw: &str) -> Result<Self, String> {
        let bytes = STANDARD
            .decode(raw.trim())
            .map_err(|_| "CENTRAL_LOGS_SESSION_KEY must be base64".to_string())?;
        let key: [u8; KEY_LEN] = bytes
            .try_into()
            .map_err(|_| "CENTRAL_LOGS_SESSION_KEY must decode to 32 bytes".to_string())?;
        Ok(Self::new(key))
    }
}

fn aad(session_id: &str) -> String {
    format!("central-logs-refresh:v1:{session_id}")
}

impl SecretSealer for AesSecretSealer {
    fn seal(&self, session_id: &str, plaintext: &str) -> Result<String, AppError> {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let encrypted = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: aad(session_id).as_bytes(),
                },
            )
            .map_err(|_| AppError::Storage("could not seal secret".into()))?;
        Ok(STANDARD.encode([nonce.as_slice(), encrypted.as_slice()].concat()))
    }

    fn unseal(&self, session_id: &str, sealed: &str) -> Result<String, AppError> {
        let error = || AppError::Storage("could not unseal secret".into());
        let bytes = STANDARD.decode(sealed).map_err(|_| error())?;
        if bytes.len() < NONCE_LEN {
            return Err(error());
        }
        let (nonce, body) = bytes.split_at(NONCE_LEN);
        let clear = self
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: body,
                    aad: aad(session_id).as_bytes(),
                },
            )
            .map_err(|_| error())?;
        String::from_utf8(clear).map_err(|_| error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_value_opens_only_for_the_same_session() {
        let sealer = AesSecretSealer::new([7; KEY_LEN]);
        let sealed = sealer.seal("session-a", "refresh-token").unwrap();

        assert!(!sealed.contains("refresh-token"));
        assert_eq!(
            sealer.unseal("session-a", &sealed).unwrap(),
            "refresh-token"
        );
        assert!(sealer.unseal("session-b", &sealed).is_err());
    }

    #[test]
    fn a_different_key_cannot_open_the_value() {
        let sealed = AesSecretSealer::new([1; KEY_LEN])
            .seal("s", "secret")
            .unwrap();

        assert!(
            AesSecretSealer::new([2; KEY_LEN])
                .unseal("s", &sealed)
                .is_err()
        );
    }

    #[test]
    fn each_seal_uses_a_fresh_nonce() {
        let sealer = AesSecretSealer::new([3; KEY_LEN]);
        assert_ne!(
            sealer.seal("s", "x").unwrap(),
            sealer.seal("s", "x").unwrap()
        );
    }

    #[test]
    fn key_must_be_base64_of_32_bytes() {
        let good = STANDARD.encode([9u8; KEY_LEN]);
        assert!(AesSecretSealer::from_base64(&good).is_ok());
        assert!(AesSecretSealer::from_base64("not base64!").is_err());
        assert!(AesSecretSealer::from_base64(&STANDARD.encode([9u8; 16])).is_err());
    }

    #[test]
    fn garbage_ciphertext_is_rejected() {
        let sealer = AesSecretSealer::new([5; KEY_LEN]);
        assert!(sealer.unseal("s", "").is_err());
        assert!(sealer.unseal("s", "AAAA").is_err());
        assert!(sealer.unseal("s", "%%%").is_err());
    }
}
