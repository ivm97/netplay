use crate::crypto::{self, key_from_password};
use crate::iface::{self, create_tun};
use crate::protocol::{
    self, MSG_DATA, MSG_GOODBYE, MSG_HANDSHAKE, MSG_HANDSHAKE_OK, MSG_KEEPALIVE, handshake_payload,
    verify_handshake,
};
use crate::stun;
use crate::wizard;
use anyhow::{Context, Result, bail};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{interval, timeout};

const KEEPALIVE_SECS: u64 = 10;
const PEER_TIMEOUT_SECS: u64 = 45;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct TunnelOpts {
    pub is_host: bool,
    pub listen: SocketAddr,
    pub peer: Option<SocketAddr>,
    pub password: String,
    pub use_stun: bool,
}

pub async fn run(opts: TunnelOpts) -> Result<()> {
    let key = key_from_password(&opts.password);
    let sock = UdpSocket::bind(opts.listen)
        .await
        .with_context(|| format!("bind {}", opts.listen))?;
    sock.set_broadcast(true)?;

    tracing::info!("UDP слушает {}", sock.local_addr()?);

    if opts.is_host {
        let port = opts.listen.port();
        if opts.use_stun {
            match stun::discover_public_addr(&sock).await {
                Ok(pub_addr) => {
                    wizard::print_host_ready(&pub_addr.to_string(), &opts.password, port);
                }
                Err(e) => {
                    tracing::warn!("STUN недоступен ({e}). Сообщите партнёру внешний IP вручную.");
                    let hint = format!("ВАШ_ВНЕШНИЙ_IP:{port}");
                    wizard::print_host_ready(&hint, &opts.password, port);
                }
            }
        } else {
            let hint = format!("ВАШ_ВНЕШНИЙ_IP:{port}");
            wizard::print_host_ready(&hint, &opts.password, port);
        }
        wait_and_run_host(sock, key).await
    } else {
        let peer = opts.peer.context("--peer обязателен для join")?;
        println!();
        println!("Подключаемся к хосту {peer}…");
        println!("  Вы в игре будете:     {}", iface::CLIENT_IP);
        println!("  Хост в игре будет:    {}", iface::HOST_IP);
        println!();
        connect_and_run_client(sock, peer, key).await
    }
}

async fn wait_and_run_host(sock: UdpSocket, key: [u8; 32]) -> Result<()> {
    let sock = Arc::new(sock);
    let mut buf = [0u8; 65535];
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;

    let peer = loop {
        if Instant::now() > deadline {
            bail!("никто не подключился за {} с", HANDSHAKE_TIMEOUT.as_secs());
        }
        let (n, from) = timeout(Duration::from_secs(5), sock.recv_from(&mut buf))
            .await
            .context("ожидание партнёра (timeout)")??;

        let Some((msg, payload)) = protocol::decode(&buf[..n]) else {
            continue;
        };
        if msg != MSG_HANDSHAKE {
            continue;
        }
        let Some((_peer_is_host, octet)) = verify_handshake(payload, &key) else {
            tracing::warn!("отклонён handshake с {from} (неверный пароль?)");
            continue;
        };
        if octet != 2 {
            tracing::warn!("неожиданный IP-октет от клиента: {octet}");
        }

        let ok = protocol::encode(MSG_HANDSHAKE_OK, &handshake_payload(true, 1, &key));
        sock.send_to(&ok, from).await?;
        tracing::info!("Партнёр подключён: {from}");
        break from;
    };

    run_forwarding(sock, peer, key, true).await
}

async fn connect_and_run_client(sock: UdpSocket, peer: SocketAddr, key: [u8; 32]) -> Result<()> {
    let sock = Arc::new(sock);
    let hs = protocol::encode(MSG_HANDSHAKE, &handshake_payload(false, 2, &key));

    let mut buf = [0u8; 2048];
    let mut ok_peer = None;

    for attempt in 1..=30 {
        sock.send_to(&hs, peer).await?;
        tracing::debug!("handshake попытка {attempt}");
        match timeout(Duration::from_secs(2), sock.recv_from(&mut buf)).await {
            Ok(Ok((n, from))) => {
                if let Some((msg, payload)) = protocol::decode(&buf[..n]) {
                    if msg == MSG_HANDSHAKE_OK {
                        if verify_handshake(payload, &key).is_some() {
                            ok_peer = Some(from);
                            break;
                        }
                    }
                }
            }
            _ => continue,
        }
    }

    let peer =
        ok_peer.context("не удалось установить соединение (проверьте IP, порт, пароль, NAT)")?;
    tracing::info!("Соединение установлено с {peer}");
    run_forwarding(sock, peer, key, false).await
}

async fn run_forwarding(
    sock: Arc<UdpSocket>,
    peer: SocketAddr,
    key: [u8; 32],
    is_host: bool,
) -> Result<()> {
    let tun = create_tun(is_host)?;
    let (mut tun_writer, mut tun_reader) = tun
        .split()
        .context("не удалось разделить TUN на reader/writer")?;
    let last_peer = Arc::new(Mutex::new(Instant::now()));
    let peer = Arc::new(Mutex::new(peer));

    println!();
    println!("Туннель активен. В игре используйте LAN / IP партнёра:");
    if is_host {
        println!("  Вы: {}   Партнёр: {}", iface::HOST_IP, iface::CLIENT_IP);
    } else {
        println!("  Вы: {}   Партнёр: {}", iface::CLIENT_IP, iface::HOST_IP);
    }
    println!("Ctrl+C — выход.");
    println!();

    let sock_r = Arc::clone(&sock);
    let peer_r = Arc::clone(&peer);
    let last_r = Arc::clone(&last_peer);
    let key_r = key;

    let udp_to_tun = async {
        let mut buf = [0u8; 65535];
        loop {
            let (n, from) = sock_r.recv_from(&mut buf).await?;
            {
                let mut p = peer_r.lock().await;
                *p = from;
            }
            *last_r.lock().await = Instant::now();

            let Some((msg, payload)) = protocol::decode(&buf[..n]) else {
                continue;
            };
            match msg {
                MSG_DATA => {
                    let plain = crypto::decrypt(&key_r, payload)?;
                    if !plain.is_empty() {
                        tun_writer.write_all(&plain).await?;
                    }
                }
                MSG_KEEPALIVE => {}
                MSG_GOODBYE => {
                    tracing::info!("партнёр отключился");
                    bail!("goodbye");
                }
                MSG_HANDSHAKE | MSG_HANDSHAKE_OK => {}
                _ => {}
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    let sock_w = Arc::clone(&sock);
    let peer_w = Arc::clone(&peer);
    let key_w = key;

    let tun_to_udp = async {
        let mut buf = [0u8; 65535];
        loop {
            let n = tun_reader.read(&mut buf).await?;
            if n == 0 {
                continue;
            }
            let enc = crypto::encrypt(&key_w, &buf[..n])?;
            let pkt = protocol::encode(MSG_DATA, &enc);
            let dest = *peer_w.lock().await;
            sock_w.send_to(&pkt, dest).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    let sock_k = Arc::clone(&sock);
    let peer_k = Arc::clone(&peer);
    let last_k = Arc::clone(&last_peer);

    let keepalive = async {
        let mut tick = interval(Duration::from_secs(KEEPALIVE_SECS));
        loop {
            tick.tick().await;
            let dest = *peer_k.lock().await;
            let pkt = protocol::encode(MSG_KEEPALIVE, &[]);
            let _ = sock_k.send_to(&pkt, dest).await;
            let elapsed = last_k.lock().await.elapsed();
            if elapsed > Duration::from_secs(PEER_TIMEOUT_SECS) {
                bail!(
                    "партнёр не отвечает {} с — соединение потеряно",
                    elapsed.as_secs()
                );
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        r = udp_to_tun => r?,
        r = tun_to_udp => r?,
        r = keepalive => r?,
        _ = tokio::signal::ctrl_c() => {
            let dest = *peer.lock().await;
            let _ = sock
                .send_to(&protocol::encode(MSG_GOODBYE, &[]), dest)
                .await;
            tracing::info!("выход");
        }
    }

    Ok(())
}
