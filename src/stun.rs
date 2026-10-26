use anyhow::{Context, Result, bail};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

const STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302",
    "stun.cloudflare.com:3478",
];

/// RFC 5389 Binding Request → mapped address.
pub async fn discover_public_addr(sock: &UdpSocket) -> Result<SocketAddr> {
    let mut last_err = None;
    for server in STUN_SERVERS {
        match try_stun(sock, server).await {
            Ok(addr) => return Ok(addr),
            Err(e) => {
                tracing::debug!("STUN {server}: {e}");
                last_err = Some(e);
            }
        }
    }
    bail!(
        "не удалось узнать публичный адрес через STUN: {}",
        last_err.map(|e| e.to_string()).unwrap_or_default()
    )
}

async fn try_stun(sock: &UdpSocket, server: &str) -> Result<SocketAddr> {
    let server: SocketAddr = tokio::net::lookup_host(server)
        .await
        .context("DNS STUN")?
        .next()
        .context("нет A/AAAA для STUN")?;

    // Binding Request: type=0x0001, length=0, magic cookie, random txid
    let mut req = [0u8; 20];
    req[0] = 0x00;
    req[1] = 0x01;
    // length already 0
    req[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
    let mut txid = [0u8; 12];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut txid);
    req[8..20].copy_from_slice(&txid);

    sock.send_to(&req, server).await?;

    let mut buf = [0u8; 512];
    let (n, _) = timeout(Duration::from_secs(2), sock.recv_from(&mut buf))
        .await
        .context("STUN timeout")??;

    parse_mapped_address(&buf[..n], &txid)
}

fn parse_mapped_address(msg: &[u8], txid: &[u8; 12]) -> Result<SocketAddr> {
    if msg.len() < 20 {
        bail!("STUN response too short");
    }
    if msg[0..2] != [0x01, 0x01] {
        bail!("not a Binding Success");
    }
    if msg[4..8] != 0x2112A442u32.to_be_bytes() {
        bail!("bad magic cookie");
    }
    if msg[8..20] != *txid {
        bail!("txid mismatch");
    }

    let mut i = 20;
    let msg_len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    let end = 20 + msg_len;
    if end > msg.len() {
        bail!("truncated length");
    }

    while i + 4 <= end {
        let atype = u16::from_be_bytes([msg[i], msg[i + 1]]);
        let alen = u16::from_be_bytes([msg[i + 2], msg[i + 3]]) as usize;
        let val_start = i + 4;
        let val_end = val_start + alen;
        if val_end > end {
            bail!("attr overflow");
        }
        // XOR-MAPPED-ADDRESS (0x0020) preferred, else MAPPED-ADDRESS (0x0001)
        if (atype == 0x0020 || atype == 0x0001) && alen >= 8 {
            let family = msg[val_start + 1];
            let mut port = u16::from_be_bytes([msg[val_start + 2], msg[val_start + 3]]);
            if atype == 0x0020 {
                port ^= 0x2112;
            }
            if family == 0x01 && alen >= 8 {
                let mut ip = [
                    msg[val_start + 4],
                    msg[val_start + 5],
                    msg[val_start + 6],
                    msg[val_start + 7],
                ];
                if atype == 0x0020 {
                    let cookie = 0x2112A442u32.to_be_bytes();
                    for (b, c) in ip.iter_mut().zip(cookie) {
                        *b ^= c;
                    }
                }
                return Ok(SocketAddr::from((ip, port)));
            }
        }
        i = val_end + (4 - alen % 4) % 4; // padding
    }
    bail!("no mapped address in STUN response")
}
