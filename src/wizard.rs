use anyhow::{Context, Result, bail};
use std::io::{self, Write};
use std::net::SocketAddr;

/// Показать обе стороны: что делает хост и что вводит гость.
pub fn print_both_sides_guide(peer_hint: &str, password_hint: &str, port: u16) {
    println!();
    println!("================================================================");
    println!("  netplay — связь двух(пока что) ПК для LAN-игр");
    println!("================================================================");
    println!();
    println!("  >>> ВЫ ХОСТ (Нужно создать комнату)");
    println!("      1. Запустите программу от имени администратора");
    println!("      2. Выберите пункт [1] и введите общий пароль");
    println!("      3. Сообщите гостю ДВЕ вещи:");
    println!("         - адрес  (пример: {peer_hint})");
    println!("         - пароль (пример: {password_hint})");
    println!("      Ваш IP в игре: 10.77.0.1");
    println!();
    println!("  >>> ВЫ ВТОРОЙ ИГРОК (гость)");
    println!("      1. Запускает программу от имени администратора");
    println!("      2. Выбирает [2] Подключиться");
    println!("      3. Вводит адрес и пароль, которые вы прислали");
    println!("      Либо одной строкой:");
    println!("         netplay join --peer {peer_hint} -p {password_hint}");
    println!("      IP друга в игре: 10.77.0.2");
    println!();
    println!("  Порт по умолчанию: {port}  (при проблемах — проброс UDP на роутере хоста)");
    println!("  Повтор прошлого раза: [3] Reconnect  или  netplay reconnect");
    println!("================================================================");
    println!();
}

pub enum WizardChoice {
    Host {
        password: String,
        port: u16,
        no_stun: bool,
    },
    Join {
        peer: SocketAddr,
        password: String,
    },
    Reconnect,
}

pub fn run_interactive() -> Result<WizardChoice> {
    print_both_sides_guide("IP_ХОСТА:51820", "ваш_пароль", 51820);

    println!("Что сделать на ЭТОМ компьютере?");
    println!("  [1] Я хост — создать комнату и ждать друга");
    println!("  [2] Я подключаюсь — у меня есть адрес и пароль от друга");
    println!("  [3] Reconnect — повторить последнее сохранение");
    println!("  [0] Выход");
    print!("Ваш выбор: ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    match line.trim() {
        "1" => {
            let password = ask("Общий пароль комнаты (минимум 4 символа, скажите его другу)")?;
            if password.len() < 4 {
                bail!("пароль слишком короткий");
            }
            let port = ask_default("UDP-порт хоста", "51820")?
                .parse::<u16>()
                .context("порт")?;
            println!();
            println!("Сейчас узнаем ваш адрес (STUN) и покажем, что именно написать другу.");
            Ok(WizardChoice::Host {
                password,
                port,
                no_stun: false,
            })
        }
        "2" => {
            println!();
            println!("Друг (хост) должен прислать вам адрес вида 1.2.3.4:51820 и пароль.");
            let peer: SocketAddr = ask("Введите адрес хоста (IP:порт)")?
                .parse()
                .context("адрес вида 203.0.113.10:51820")?;
            let password = ask("Введите пароль, который сказал хост")?;
            if password.len() < 4 {
                bail!("пароль слишком короткий");
            }
            Ok(WizardChoice::Join { peer, password })
        }
        "3" => Ok(WizardChoice::Reconnect),
        "0" | "" => bail!("выход"),
        other => bail!("неизвестный пункт: {other}"),
    }
}

fn ask(prompt: &str) -> Result<String> {
    print!("{prompt}: ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let s = line.trim().to_string();
    if s.is_empty() {
        bail!("пустой ввод");
    }
    Ok(s)
}

fn ask_default(prompt: &str, default: &str) -> Result<String> {
    print!("{prompt} [{default}]: ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let s = line.trim();
    Ok(if s.is_empty() {
        default.to_string()
    } else {
        s.to_string()
    })
}

/// Баннер после STUN / готовности хоста — обе подсказки с реальными данными.
pub fn print_host_ready(pub_addr: &str, password: &str, listen_port: u16) {
    println!();
    println!("================================================================");
    println!("  ХОСТ ГОТОВ — скопируйте другу блок ниже");
    println!("================================================================");
    println!();
    println!("  --- Что СООБЩИТЬ другу ---");
    println!("  Адрес:  {pub_addr}");
    println!("  Пароль: {password}");
    println!();
    println!("  Команда для друга (можно вставить целиком):");
    println!("    netplay join --peer {pub_addr} -p {password}");
    println!();
    println!("  --- Что ВВЕСТИ другу в мастере netplay ---");
    println!("  1) Запустить netplay от администратора");
    println!("  2) Выбрать [2] Подключиться");
    println!("  3) Адрес:  {pub_addr}");
    println!("  4) Пароль: {password}");
    println!();
    println!("  Вы (хост) в игре:     10.77.0.1");
    println!("  Друг (клиент) в игре: 10.77.0.2");
    println!();
    println!("  Если друг не подключается — на роутере пробросьте UDP {listen_port} на этот ПК.");
    println!("  Ждём подключение друга…");
    println!();
}

pub fn pause_if_console() {
    println!();
    print!("Нажмите Enter, чтобы закрыть окно… ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}
