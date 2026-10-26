use anyhow::{Context, Result};
use std::net::Ipv4Addr;
use tun::Configuration;

pub const HOST_IP: Ipv4Addr = Ipv4Addr::new(10, 77, 0, 1);
pub const CLIENT_IP: Ipv4Addr = Ipv4Addr::new(10, 77, 0, 2);
pub const NETMASK: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 0);
pub const MTU: u16 = 1400;

pub fn virtual_ip(is_host: bool) -> Ipv4Addr {
    if is_host { HOST_IP } else { CLIENT_IP }
}

pub fn create_tun(is_host: bool) -> Result<tun::AsyncDevice> {
    let addr = virtual_ip(is_host);
    let mut config = Configuration::default();
    config.address(addr).netmask(NETMASK).mtu(MTU).up();

    #[cfg(target_os = "linux")]
    {
        config.platform_config(|p| {
            p.ensure_root_privileges(true);
        });
    }

    #[cfg(target_os = "windows")]
    {
        // Имя адаптера видно в «Сетевые подключения» Windows
        config.name("netplay");
        config.platform_config(|p| {
            p.device_guid(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        });
    }

    let device = tun::create_as_async(&config).context(
        "не удалось создать TUN (нужны права администратора; на Windows — wintun.dll рядом с exe)",
    )?;

    tracing::info!(
        "Виртуальный адаптер готов: {}/24 (партнёр: {})",
        addr,
        if is_host { CLIENT_IP } else { HOST_IP }
    );
    Ok(device)
}
