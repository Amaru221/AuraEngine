//! Downloads and installs a portable mpv build on first use.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::Sender;

#[derive(Clone, Debug)]
pub enum InstallEvent {
    Stage(String),
    Progress { downloaded: u64, total: u64 },
    Done(PathBuf),
    Failed(String),
}

pub fn install_mpv(tx: Sender<InstallEvent>) {
    if let Err(err) = install_mpv_inner(&tx) {
        let _ = tx.send(InstallEvent::Failed(format!("{err:#}")));
    }
}

fn install_mpv_inner(tx: &Sender<InstallEvent>) -> Result<PathBuf> {
    let _ = tx.send(InstallEvent::Stage("Contacting GitHub".into()));
    let client = reqwest::blocking::Client::builder()
        .user_agent("AuraEngine/1.0 (Windows)")
        .timeout(Duration::from_secs(45))
        .build()?;

    let release: serde_json::Value = client
        .get("https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest")
        .send()
        .context("query mpv releases")?
        .error_for_status()?
        .json()
        .context("parse release json")?;

    let mut candidates: Vec<(String, String)> = Vec::new();
    if let Some(assets) = release.get("assets").and_then(|a| a.as_array()) {
        for asset in assets {
            let name = asset
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            let url = asset
                .get("browser_download_url")
                .and_then(|u| u.as_str())
                .unwrap_or_default();
            if !name.starts_with("mpv-")
                || !name.ends_with(".7z")
                || !name.contains("x86_64")
                || name.contains("dev")
            {
                continue;
            }
            candidates.push((name.to_string(), url.to_string()));
        }
    }
    // Prefer the baseline build; the v3 build requires a newer CPU.
    candidates.sort_by_key(|(name, _)| name.contains("v3"));
    let (asset_name, asset_url) = candidates
        .first()
        .cloned()
        .context("no x86_64 mpv asset found in the latest release")?;

    let _ = tx.send(InstallEvent::Stage(format!("Downloading {asset_name}")));
    let download_dir = crate::util::data_root().join("downloads");
    std::fs::create_dir_all(&download_dir).ok();
    let archive = download_dir.join("mpv.7z");
    let mut response = client
        .get(&asset_url)
        .send()
        .context("download mpv")?
        .error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(&archive).context("create archive")?;
    let mut buffer = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let read = response.read(&mut buffer).context("read mpv archive")?;
        if read == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buffer[..read])?;
        downloaded += read as u64;
        let _ = tx.send(InstallEvent::Progress { downloaded, total });
    }
    drop(file);

    let _ = tx.send(InstallEvent::Stage("Extracting".into()));
    let extract_dir = crate::util::data_root().join("downloads").join("mpv-extract");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir)?;
    sevenz_rust2::decompress_file(&archive, &extract_dir).context("extract 7z archive")?;

    let exe = find_file(&extract_dir, "mpv.exe").context("mpv.exe not found in archive")?;
    let target = crate::util::bin_dir().join("mpv.exe");
    std::fs::create_dir_all(crate::util::bin_dir())?;
    std::fs::copy(&exe, &target).context("install mpv.exe")?;

    let _ = std::fs::remove_dir_all(&extract_dir);
    let _ = std::fs::remove_file(&archive);

    let _ = tx.send(InstallEvent::Done(target.clone()));
    Ok(target)
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.file_name().to_string_lossy().eq_ignore_ascii_case(name)
        {
            return Some(entry.path().to_path_buf());
        }
    }
    None
}
