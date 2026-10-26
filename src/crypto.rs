use anyhow::{Result, bail};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand::RngCore;

const NONCE_LEN: usize = 12;
const TAG_OVERHEAD: usize = 16;

/// Ключ из пароля комнаты (одинаковый у обоих пиров).
pub fn key_from_password(password: &str) -> [u8; 32] {
    blake3::derive_key("netplay-lan-v1", password.as_bytes())
}

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);
    let mut out = Vec::with_capacity(NONCE_LEN + plaintext.len() + TAG_OVERHEAD);
    out.extend_from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN + TAG_OVERHEAD {
        bail!("ciphertext too short");
    }
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::try_from(&data[..NONCE_LEN])
        .map_err(|_| anyhow::anyhow!("invalid nonce length"))?;
    cipher
        .decrypt(&nonce, &data[NONCE_LEN..])
        .map_err(|_| anyhow::anyhow!("decrypt failed (wrong password or corrupt packet)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = key_from_password("secret");
        let enc = encrypt(&key, b"hello lan").unwrap();
        let dec = decrypt(&key, &enc).unwrap();
        assert_eq!(dec, b"hello lan");
    }
}
