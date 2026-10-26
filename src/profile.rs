use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Host,
    Join,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedProfile {
    pub role: Role,
    pub password: String,
    /// Порт прослушивания (host) или локальный bind-порт (join, часто 0).
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub no_stun: bool,
    /// Адрес хоста — только для Join.
    pub peer: Option<SocketAddr>,
}

fn default_bind() -> String {
    "0.0.0.0".into()
}

fn profile_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("NETPLAY_PROFILE") {
        return Ok(PathBuf::from(p));
    }
    let base = dirs::config_dir().context("не найден каталог конфигов ОС")?;
    Ok(base.join("netplay").join("profile.json"))
}

pub fn save(profile: &SavedProfile) -> Result<()> {
    let path = profile_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(profile)?;
    fs::write(&path, json).with_context(|| format!("запись {}", path.display()))?;
    tracing::info!("Профиль сохранён: {}", path.display());
    println!("Профиль сохранён — в следующий раз: netplay reconnect");
    Ok(())
}

pub fn load() -> Result<SavedProfile> {
    let path = profile_path()?;
    if !path.is_file() {
        bail!(
            "сохранённого подключения нет ({})\nСначала выполните: netplay host … или netplay join …",
            path.display()
        );
    }
    let data = fs::read_to_string(&path).with_context(|| format!("чтение {}", path.display()))?;
    let profile: SavedProfile = serde_json::from_str(&data).context("разбор profile.json")?;
    Ok(profile)
}

pub fn show_path() -> Result<PathBuf> {
    profile_path()
}
