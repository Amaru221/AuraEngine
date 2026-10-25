//! Online wallpaper catalogs and downloads.

mod motionbgs;

pub use motionbgs::MotionBgsCategory;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::MediaKind;
use crate::util;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Wallhaven,
    Bing,
    MotionBgs,
    Pexels,
}

impl Provider {
    pub fn all() -> [Provider; 4] {
        [
            Provider::Wallhaven,
            Provider::Bing,
            Provider::MotionBgs,
            Provider::Pexels,
        ]
    }

    /// Whether the UI should offer a search box for this provider.
    pub fn supports_search(self) -> bool {
        !matches!(self, Provider::Bing)
    }

    pub fn supports_pages(self) -> bool {
        !matches!(self, Provider::Bing)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteItem {
    pub id: String,
    pub title: String,
    pub thumb_url: String,
    /// Cached SHA-256 of `thumb_url`, computed once when the item is parsed.
    pub thumb_key: String,
    pub full_url: String,
    pub page_url: String,
    pub kind: MediaKind,
    pub source: String,
    pub resolution: String,
}

impl RemoteItem {
    /// Stable cache key for the remote thumbnail.
    pub fn thumb_key(&self) -> &str {
        &self.thumb_key
    }
}

/// Typed catalog failure. The UI uses this to tell an IP/Cloudflare block
/// apart from a network outage or an empty search.
#[derive(Clone, Debug)]
pub enum SearchError {
    /// The provider (currently MotionBGS) refused the connection at the edge.
    Blocked { status: u16, detail: String },
    RateLimited { retry_after: Option<u64> },
    Offline(String),
    Empty(String),
    Other(String),
}

impl SearchError {
    /// Small technical detail suitable for logs, not for the main UI title.
    pub fn detail(&self) -> Option<&str> {
        match self {
            SearchError::Blocked { detail, .. } if !detail.is_empty() => Some(detail),
            SearchError::Offline(detail)
            | SearchError::Empty(detail)
            | SearchError::Other(detail)
                if !detail.is_empty() =>
            {
                Some(detail)
            }
            _ => None,
        }
    }
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::Blocked { status, detail } => {
                write!(f, "catalog blocked the connection (HTTP {status})")?;
                if !detail.is_empty() {
                    write!(f, ": {detail}")?;
                }
                Ok(())
            }
            SearchError::RateLimited { retry_after } => match retry_after {
                Some(seconds) => write!(f, "catalog rate limit; retry in {seconds}s"),
                None => write!(f, "catalog rate limit"),
            },
            SearchError::Offline(detail) => write!(f, "could not connect: {detail}"),
            SearchError::Empty(detail) => write!(f, "no results: {detail}"),
            SearchError::Other(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<anyhow::Error> for SearchError {
    fn from(err: anyhow::Error) -> Self {
        SearchError::Other(format!("{err:#}"))
    }
}

/// One HTTP client for the whole process: reusing the TLS session and
/// connection pool keeps catalog pages and thumbnails from paying the
/// handshake cost on every request.
pub fn http_client() -> Result<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .user_agent("AuraEngine/1.0 (Windows live wallpaper manager)")
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|err| format!("{err:#}"))
        })
        .as_ref()
        .map_err(|err| anyhow!("build http client: {err}"))
}

fn client() -> Result<&'static reqwest::blocking::Client> {
    http_client()
}

pub fn search(
    provider: Provider,
    query: &str,
    page: u32,
    api_key: &str,
    category: MotionBgsCategory,
    force: bool,
) -> std::result::Result<Vec<RemoteItem>, SearchError> {
    match provider {
        Provider::Wallhaven => {
            wallhaven(query, page, api_key).map_err(SearchError::from)
        }
        Provider::Bing => bing().map_err(SearchError::from),
        Provider::MotionBgs => motionbgs::search(query, page, category, force),
        Provider::Pexels => pexels(query, page, api_key).map_err(SearchError::from),
    }
}

/// Browser URL for the current MotionBGS search/category, used by the
/// "Open in browser" action when the app connection is blocked.
pub fn motionbgs_page_url(
    query: &str,
    page: u32,
    category: MotionBgsCategory,
) -> String {
    motionbgs::build_url(query, page, category)
}

fn wallhaven(query: &str, page: u32, api_key: &str) -> Result<Vec<RemoteItem>> {
    let mut url = format!(
        "https://wallhaven.cc/api/v1/search?sorting=toplist&purity=100&categories=111&atleast=1920x1080&page={}",
        page.max(1)
    );
    if !query.trim().is_empty() {
        url.push_str("&q=");
        url.push_str(&urlencoding::encode(query.trim()));
    }
    if !api_key.trim().is_empty() {
        url.push_str("&apikey=");
        url.push_str(&urlencoding::encode(api_key.trim()));
    }

    let json: serde_json::Value = client()?
        .get(&url)
        .send()?
        .error_for_status()?
        .json()
        .context("parse wallhaven response")?;

    let mut out = Vec::new();
    for item in json
        .get("data")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
    {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("wallhaven")
            .to_string();
        let thumb = item
            .get("thumbs")
            .and_then(|t| t.get("large").or_else(|| t.get("original")))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let full = item
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if full.is_empty() {
            continue;
        }
        let resolution = item
            .get("resolution")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let thumb_key = util::cache_key_for_text(&thumb);
        out.push(RemoteItem {
            title: format!("Wallhaven {id}"),
            thumb_url: thumb,
            thumb_key,
            full_url: full,
            page_url: item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://wallhaven.cc")
                .to_string(),
            kind: MediaKind::Image,
            source: "Wallhaven".into(),
            resolution,
            id,
        });
    }
    Ok(out)
}

fn bing() -> Result<Vec<RemoteItem>> {
    let url = "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=8&mkt=en-US";
    let json: serde_json::Value = client()?
        .get(url)
        .send()?
        .error_for_status()?
        .json()
        .context("parse bing response")?;

    let mut out = Vec::new();
    for item in json
        .get("images")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
    {
        let base = item
            .get("urlbase")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if base.is_empty() {
            continue;
        }
        let start = item
            .get("startdate")
            .and_then(|v| v.as_str())
            .unwrap_or("bing")
            .to_string();
        let thumb_url = format!("https://www.bing.com{base}_400x240.jpg");
        let thumb_key = util::cache_key_for_text(&thumb_url);
        out.push(RemoteItem {
            id: start.clone(),
            title: item
                .get("title")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("Bing Daily")
                .to_string(),
            thumb_url,
            thumb_key,
            full_url: format!("https://www.bing.com{base}_UHD.jpg"),
            page_url: "https://www.bing.com".into(),
            kind: MediaKind::Image,
            source: "Bing".into(),
            resolution: "3840x2160".into(),
        });
    }
    Ok(out)
}

fn pexels(query: &str, page: u32, api_key: &str) -> Result<Vec<RemoteItem>> {
    if api_key.trim().is_empty() {
        return Err(anyhow!("A Pexels API key is required (Settings > Online)"));
    }
    let q = if query.trim().is_empty() {
        "live wallpaper"
    } else {
        query.trim()
    };
    let url = format!(
        "https://api.pexels.com/videos/search?query={}&per_page=24&page={}",
        urlencoding::encode(q),
        page.max(1)
    );
    let json: serde_json::Value = client()?
        .get(&url)
        .header("Authorization", api_key.trim())
        .send()?
        .error_for_status()?
        .json()
        .context("parse pexels response")?;

    let mut out = Vec::new();
    for item in json
        .get("videos")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
    {
        let id = item
            .get("id")
            .and_then(|v| v.as_i64())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "pexels".into());
        let mut best: Option<(u32, String, u32, u32)> = None;
        for file in item
            .get("video_files")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let link = file
                .get("link")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let width = file.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let height = file.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            if link.is_empty() || width == 0 || width > 2560 {
                continue;
            }
            if best.as_ref().map(|(w, _, _, _)| width > *w).unwrap_or(true) {
                best = Some((width, link, width, height));
            }
        }
        let Some((_, link, width, height)) = best else {
            continue;
        };
        let thumb_url = item
            .get("image")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let thumb_key = util::cache_key_for_text(&thumb_url);
        out.push(RemoteItem {
            title: format!("Pexels {id}"),
            thumb_url,
            thumb_key,
            full_url: link,
            page_url: item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://www.pexels.com")
                .to_string(),
            kind: MediaKind::Video,
            source: "Pexels".into(),
            resolution: format!("{width}x{height}"),
            id,
        });
    }
    Ok(out)
}

/// Downloads a remote item into the Aura online library.
pub fn download(item: &RemoteItem, dest_dir: &Path, progress: Option<&dyn Fn(u64, u64)>) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir).ok();
    let ext = extension_from_url(&item.full_url).unwrap_or_else(|| match item.kind {
        MediaKind::Video => "mp4".into(),
        MediaKind::Image => "jpg".into(),
    });
    let filename = format!(
        "{}-{}.{}",
        util::sanitize_filename(&item.title),
        &item.id.chars().take(24).collect::<String>(),
        ext
    );
    let target = dest_dir.join(filename);
    if target.is_file() && std::fs::metadata(&target).map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(target);
    }

    let mut response = client()?.get(&item.full_url).send()?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let temp = target.with_extension("part");
    let mut file = std::fs::File::create(&temp)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    use std::io::{Read, Write};
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        downloaded += read as u64;
        if let Some(cb) = progress {
            cb(downloaded, total);
        }
    }
    drop(file);
    std::fs::rename(&temp, &target)?;
    Ok(target)
}

fn extension_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    if (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(ext)
    } else {
        None
    }
}

/// Downloads only the thumbnail bytes for the remote grid.
#[allow(dead_code)]
pub fn download_thumb(item: &RemoteItem, key: &str) -> Option<PathBuf> {
    if item.thumb_url.is_empty() {
        return None;
    }
    let path = util::thumbs_dir().join(format!("{key}.jpg"));
    if path.is_file() {
        return Some(path);
    }
    std::fs::create_dir_all(util::thumbs_dir()).ok()?;
    let bytes = client()
        .ok()?
        .get(&item.thumb_url)
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let thumb = image.thumbnail(480, 270);
    thumb.save_with_format(&path, image::ImageFormat::Jpeg).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_extensions() {
        assert_eq!(
            extension_from_url("https://x/y/image.JPG?token=1"),
            Some("jpg".into())
        );
        assert_eq!(extension_from_url("https://x/y"), None);
    }
}

