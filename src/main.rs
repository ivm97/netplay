mod crypto;
mod iface;
mod profile;
mod protocol;
mod stun;
mod tunnel;
mod wintun_fetch;
mod wizard;

use anyhow::Result;
use clap::{Parser, Subcommand};
use profile::{Role, SavedProfile};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;
use wizard::WizardChoice;

#[derive(Parser, Debug)]
#[command(
    name = "netplay",
    version,
    about = "Виртуальная LAN 1↔1 для сетевых игр без игрового сервера",
    long_about = "Создаёт зашифрованный туннель между двумя Windows/Linux ПК.\n\
Игры видят общую подсеть 10.77.0.0/24 (хост .1, клиент .2).\n\
Запуск без аргументов открывает мастер с подсказками для хоста и гостя.\n\
На Windows при отсутствии wintun.dll она скачивается автоматически с wintun.net."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Создать комнату (вы — 10.77.0.1)
    Host {
        #[arg(long, default_value = "51820")]
        port: u16,
        #[arg(long, short = 'p')]
        password: String,
        #[arg(long)]
        no_stun: bool,
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
        #[arg(long)]
        no_save: bool,
    },
    /// Подключиться к хосту (вы — 10.77.0.2)
    Join {
        #[arg(long)]
        peer: SocketAddr,
        #[arg(long, short = 'p')]
        password: String,
        #[arg(long, default_value = "0")]
        port: u16,
        #[arg(long)]
        no_save: bool,
    },
    /// Повторить последнее сохранённое подключение
    Reconnect,
}

#[tokio::main]
async fn main() {
    let result = real_main().await;
    if let Err(e) = result {
        eprintln!("Ошибка: {e:#}");
        wizard::pause_if_console();
        std::process::exit(1);
    }
}

async fn real_main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    #[cfg(target_os = "windows")]
    tracing::info!("Windows: лучше запускать от имени администратора");

    wintun_fetch::ensure_wintun()?;

    match cli.cmd {
        None => {
            // Двойной клик / запуск без аргументов — мастер с двумя подсказками
            match wizard::run_interactive()? {
                WizardChoice::Host {
                    password,
                    port,
                    no_stun,
                } => {
                    run_host(port, password, no_stun, "0.0.0.0".into(), false).await?;
                }
                WizardChoice::Join { peer, password } => {
                    run_join(peer, password, 0, false).await?;
                }
                WizardChoice::Reconnect => run_reconnect().await?,
            }
        }
        Some(Cmd::Host {
            port,
            password,
            no_stun,
            bind,
            no_save,
        }) => run_host(port, password, no_stun, bind, no_save).await?,
        Some(Cmd::Join {
            peer,
            password,
            port,
            no_save,
        }) => run_join(peer, password, port, no_save).await?,
        Some(Cmd::Reconnect) => run_reconnect().await?,
    }
    Ok(())
}

async fn run_host(
    port: u16,
    password: String,
    no_stun: bool,
    bind: String,
    no_save: bool,
) -> Result<()> {
    ensure_password(&password)?;
    if !no_save {
        profile::save(&SavedProfile {
            role: Role::Host,
            password: password.clone(),
            port,
            bind: bind.clone(),
            no_stun,
            peer: None,
        })?;
    }
    let listen: SocketAddr = format!("{bind}:{port}").parse()?;
    tunnel::run(tunnel::TunnelOpts {
        is_host: true,
        listen,
        peer: None,
        password,
        use_stun: !no_stun,
    })
    .await
}

async fn run_join(peer: SocketAddr, password: String, port: u16, no_save: bool) -> Result<()> {
    ensure_password(&password)?;
    if !no_save {
        profile::save(&SavedProfile {
            role: Role::Join,
            password: password.clone(),
            port,
            bind: "0.0.0.0".into(),
            no_stun: true,
            peer: Some(peer),
        })?;
    }
    let listen: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    tunnel::run(tunnel::TunnelOpts {
        is_host: false,
        listen,
        peer: Some(peer),
        password,
        use_stun: false,
    })
    .await
}

async fn run_reconnect() -> Result<()> {
    let p = profile::load()?;
    ensure_password(&p.password)?;
    println!(
        "Reconnect из {} ({:?})",
        profile::show_path()?.display(),
        p.role
    );
    match p.role {
        Role::Host => {
            run_host(p.port, p.password, p.no_stun, p.bind, true).await?;
        }
        Role::Join => {
            let peer = p.peer.ok_or_else(|| {
                anyhow::anyhow!("в профиле Join нет peer — сохраните join заново")
            })?;
            run_join(peer, p.password, p.port, true).await?;
        }
    }
    Ok(())
}

fn ensure_password(password: &str) -> Result<()> {
    if password.len() < 4 {
        anyhow::bail!("пароль слишком короткий (минимум 4 символа)");
    }
    Ok(())
}
