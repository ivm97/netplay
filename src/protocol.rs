pub const MAGIC: [u8; 4] = *b"O2O1";
pub const VERSION: u8 = 1;

pub const MSG_HANDSHAKE: u8 = 1;
pub const MSG_HANDSHAKE_OK: u8 = 2;
pub const MSG_DATA: u8 = 3;
pub const MSG_KEEPALIVE: u8 = 4;
pub const MSG_GOODBYE: u8 = 5;

/// Заголовок: magic(4) + version(1) + type(1) = 6 байт, дальше полезная нагрузка.
pub fn encode(msg_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(6 + payload.len());
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(msg_type);
    buf.extend_from_slice(payload);
    buf
}

pub fn decode(packet: &[u8]) -> Option<(u8, &[u8])> {
    if packet.len() < 6 {
        return None;
    }
    if packet[0..4] != MAGIC || packet[4] != VERSION {
        return None;
    }
    Some((packet[5], &packet[6..]))
}

/// Handshake payload: role(1) + virtual_ip_octet(1) + password_proof(32)
/// proof = blake3(password_key || "handshake")
pub fn handshake_payload(is_host: bool, ip_octet: u8, key: &[u8; 32]) -> Vec<u8> {
    let mut h = blake3::Hasher::new();
    h.update(key);
    h.update(b"handshake");
    let proof = h.finalize();
    let mut out = Vec::with_capacity(34);
    out.push(if is_host { 1 } else { 2 });
    out.push(ip_octet);
    out.extend_from_slice(proof.as_bytes());
    out
}

pub fn verify_handshake(payload: &[u8], key: &[u8; 32]) -> Option<(bool, u8)> {
    if payload.len() != 34 {
        return None;
    }
    let mut h = blake3::Hasher::new();
    h.update(key);
    h.update(b"handshake");
    let expected = h.finalize();
    if payload[2..34] != *expected.as_bytes() {
        return None;
    }
    let is_host = payload[0] == 1;
    let octet = payload[1];
    Some((is_host, octet))
}
