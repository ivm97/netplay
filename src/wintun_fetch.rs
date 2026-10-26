#[cfg(target_os = "windows")]
mod imp {
    use anyhow::{Context, Result, bail};
    use sha2::{Digest, Sha256};
    use std::fs::{self, File};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};

    const WINTUN_ZIP_URL: &str = "https://www.wintun.net/builds/wintun-0.14.1.zip";
    const WINTUN_ZIP_SHA256: &str =
        "07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51";

    fn arch_dir() -> Result<&'static str> {
        Ok(match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "x86" => "x86",
            "aarch64" => "arm64",
            other => bail!("неподдерживаемая архитектура для Wintun: {other}"),
        })
    }

    fn exe_dir() -> Result<PathBuf> {
        let exe = std::env::current_exe().context("current_exe")?;
        Ok(exe.parent().context("нет каталога exe")?.to_path_buf())
    }

    pub fn ensure_wintun() -> Result<()> {
        let dir = exe_dir()?;
        let dll = dir.join("wintun.dll");
        if dll.is_file() {
            tracing::info!("wintun.dll найдена: {}", dll.display());
            return Ok(());
        }

        println!("wintun.dll не найдена — скачиваю с https://www.wintun.net/ …");
        let zip_bytes = download_zip()?;
        verify_sha256(&zip_bytes)?;
        extract_dll(&zip_bytes, &dll)?;
        println!("Готово: {}", dll.display());
        Ok(())
    }

    fn download_zip() -> Result<Vec<u8>> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .user_agent(concat!("netplay/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let resp = client
            .get(WINTUN_ZIP_URL)
            .send()
            .context("скачивание Wintun")?
            .error_for_status()
            .context("HTTP ошибка при скачивании Wintun")?;
        let bytes = resp.bytes().context("чтение тела ответа")?;
        Ok(bytes.to_vec())
    }

    fn verify_sha256(data: &[u8]) -> Result<()> {
        let hash = Sha256::digest(data);
        let hex = hex::encode(hash);
        if hex != WINTUN_ZIP_SHA256 {
            bail!(
                "SHA-256 архива Wintun не совпал (ожидалось {}, получено {})",
                WINTUN_ZIP_SHA256,
                hex
            );
        }
        Ok(())
    }

    fn extract_dll(zip_bytes: &[u8], dest: &Path) -> Result<()> {
        let arch = arch_dir()?;
        let want = format!("wintun/bin/{arch}/wintun.dll");
        let cursor = std::io::Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).context("открытие ZIP Wintun")?;

        let mut found = None;
        for i in 0..archive.len() {
            let name = archive.by_index(i)?.name().replace('\\', "/");
            if name.eq_ignore_ascii_case(&want) {
                found = Some(i);
                break;
            }
        }
        let idx = found.with_context(|| format!("в архиве нет {want}"))?;
        let mut file = archive.by_index(idx)?;
        let mut dll_bytes = Vec::new();
        file.read_to_end(&mut dll_bytes)?;

        let tmp = dest.with_extension("dll.partial");
        {
            let mut out =
                File::create(&tmp).with_context(|| format!("создание {}", tmp.display()))?;
            out.write_all(&dll_bytes)?;
            out.sync_all()?;
        }
        fs::rename(&tmp, dest).with_context(|| format!("перемещение в {}", dest.display()))?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub use imp::ensure_wintun;

#[cfg(not(target_os = "windows"))]
pub fn ensure_wintun() -> anyhow::Result<()> {
    Ok(())
}
