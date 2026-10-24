//! Update checking and installer download from GitHub Releases.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::util;

const API_URL: &str = "https://api.github.com/repos/Amaru221/AuraEngine/releases/latest";
const CHECK_INTERVAL_SECS: u64 = 6 * 60 * 60;
const USER_AGENT: &str = concat!("AuraEngine/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    /// Version without the leading `v`, e.g. `1.0.12`.
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub page_url: String,
    pub setup_url: Option<String>,
    pub setup_sha256_url: Option<String>,
}

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    Checked(Result<Option<UpdateInfo>, String>),
    Progress { downloaded: u64, total: u64 },
    Downloaded(Result<PathBuf, String>),
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct UpdateCache {
    pub checked_at: u64,
    pub version: Option<String>,
    pub tag: Option<String>,
    pub notes: Option<String>,
    pub page_url: Option<String>,
    pub setup_url: Option<String>,
    pub setup_sha256_url: Option<String>,
}

impl UpdateCache {
    pub fn path() -> PathBuf {
        util::data_root().join("update.json")
    }

    pub fn load() -> Option<Self> {
        let raw = std::fs::read_to_string(Self::path()).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn is_fresh(&self) -> bool {
        now_secs().saturating_sub(self.checked_at) < CHECK_INTERVAL_SECS
    }

    pub fn record(info: Option<&UpdateInfo>) -> Self {
        Self {
            checked_at: now_secs(),
            version: info.map(|i| i.version.clone()),
            tag: info.map(|i| i.tag.clone()),
            notes: info.map(|i| i.notes.clone()),
            page_url: info.map(|i| i.page_url.clone()),
            setup_url: info.and_then(|i| i.setup_url.clone()),
            setup_sha256_url: info.and_then(|i| i.setup_sha256_url.clone()),
        }
    }

    pub fn info(&self) -> Option<UpdateInfo> {
        Some(UpdateInfo {
            version: self.version.clone()?,
            tag: self.tag.clone()?,
            notes: self.notes.clone().unwrap_or_default(),
            page_url: self.page_url.clone()?,
            setup_url: self.setup_url.clone(),
            setup_sha256_url: self.setup_sha256_url.clone(),
        })
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    html_url: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .context("create HTTP client")
}

/// Queries the latest release and returns it only when it is newer than this
/// build. A missing installer/checksum asset does not fail: the About page can
/// still link to the release.
pub fn check_latest() -> std::result::Result<Option<UpdateInfo>, String> {
    check_inner().map_err(|err| format!("{err:#}"))
}

fn check_inner() -> Result<Option<UpdateInfo>> {
    let release: GhRelease = client()?
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("query GitHub releases")?
        .error_for_status()
        .context("GitHub releases request")?
        .json()
        .context("parse GitHub release")?;

    let version = release.tag_name.trim_start_matches('v').trim().to_string();
    if !is_newer(&version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let setup_name = format!("aura-v{version}-windows-x64-setup.exe");
    let setup = release
        .assets
        .iter()
        .find(|asset| asset.name == setup_name)
        .map(|asset| asset.browser_download_url.clone());
    let sha = release
        .assets
        .iter()
        .find(|asset| asset.name == format!("{setup_name}.sha256"))
        .map(|asset| asset.browser_download_url.clone());

    Ok(Some(UpdateInfo {
        version,
        tag: release.tag_name,
        notes: release.body.unwrap_or_default(),
        page_url: release.html_url,
        setup_url: setup,
        setup_sha256_url: sha,
    }))
}

/// Downloads the installer, checks it against the published SHA-256 and
/// returns the path of the verified executable. Progress is sent through `tx`.
pub fn download_and_verify(
    info: &UpdateInfo,
    tx: &crossbeam_channel::Sender<UpdateEvent>,
) -> std::result::Result<PathBuf, String> {
    download_inner(info, tx).map_err(|err| format!("{err:#}"))
}

fn download_inner(
    info: &UpdateInfo,
    tx: &crossbeam_channel::Sender<UpdateEvent>,
) -> Result<PathBuf> {
    let setup_url = info
        .setup_url
        .as_ref()
        .ok_or_else(|| anyhow!("the release has no Windows installer"))?;
    let sha_url = info
        .setup_sha256_url
        .as_ref()
        .ok_or_else(|| anyhow!("the release has no SHA-256 checksum"))?;

    let dir = util::data_root().join("updates");
    std::fs::create_dir_all(&dir).context("create updates folder")?;
    let dest = dir.join(format!("aura-v{}-windows-x64-setup.exe", info.version));
    let part = dir.join(format!("aura-v{}-windows-x64-setup.exe.part", info.version));

    let client = client()?;
    let mut response = client
        .get(setup_url)
        .send()
        .context("download installer")?
        .error_for_status()
        .context("installer download")?;
    let total = response.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(&part).context("create installer file")?;
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = response.read(&mut buffer).context("read installer")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).context("write installer")?;
        downloaded += read as u64;
        let _ = tx.send(UpdateEvent::Progress { downloaded, total });
    }
    drop(file);

    let expected = client
        .get(sha_url)
        .send()
        .context("download checksum")?
        .error_for_status()
        .context("checksum request")?
        .text()
        .context("read checksum")?;
    let expected = expected
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if expected.len() != 64 {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!("invalid SHA-256 checksum file"));
    }

    let actual = sha256_file(&part)?;
    if actual != expected {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!(
            "installer checksum mismatch (expected {expected}, got {actual})"
        ));
    }

    std::fs::rename(&part, &dest).context("finalize installer")?;
    Ok(dest)
}

fn sha256_file(path: &std::path::Path) -> Result<String> {
    let mut file = std::fs::File::open(path).context("open installer for checksum")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).context("read installer checksum")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!(
        "{}",
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

/// Compares dotted numeric versions, ignoring a leading `v` and pre-release
/// suffixes (`1.2.3-beta` compares as `1.2.3`).
pub fn is_newer(remote: &str, current: &str) -> bool {
    let Some(remote) = parse_version(remote) else {
        return false;
    };
    let Some(current) = parse_version(current) else {
        return false;
    };
    let len = remote.len().max(current.len());
    for index in 0..len {
        let a = remote.get(index).copied().unwrap_or(0);
        let b = current.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

fn parse_version(version: &str) -> Option<Vec<u64>> {
    let version = version.trim().trim_start_matches('v');
    let core = version.split(['-', '+']).next()?;
    core.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_dotted_versions() {
        assert!(is_newer("1.0.12", "1.0.11"));
        assert!(is_newer("v1.1.0", "1.0.99"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(!is_newer("1.0.11", "1.0.11"));
        assert!(!is_newer("1.0.9", "1.0.10"));
        assert!(!is_newer("v2.0.0-beta", "2.0.0"));
    }
}
