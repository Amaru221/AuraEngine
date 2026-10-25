//! MotionBGS catalog (https://motionbgs.com).
//!
//! The site has no public JSON API, so the catalog is scraped from the same
//! HTML cards used by the browser. This mirrors the approach of the original
//! Linux Aura client.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::{RemoteItem, SearchError};
use crate::config::MediaKind;

const BASE: &str = "https://motionbgs.com";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MotionBgsCategory {
    #[default]
    All,
    Anime,
    Games,
    Ultra4k,
    Superhero,
    Nature,
}

impl MotionBgsCategory {
    pub fn all() -> [MotionBgsCategory; 6] {
        [
            MotionBgsCategory::All,
            MotionBgsCategory::Anime,
            MotionBgsCategory::Games,
            MotionBgsCategory::Ultra4k,
            MotionBgsCategory::Superhero,
            MotionBgsCategory::Nature,
        ]
    }

    pub fn id(self) -> &'static str {
        match self {
            MotionBgsCategory::All => "all",
            MotionBgsCategory::Anime => "anime",
            MotionBgsCategory::Games => "games",
            MotionBgsCategory::Ultra4k => "4k",
            MotionBgsCategory::Superhero => "superhero",
            MotionBgsCategory::Nature => "nature",
        }
    }

    fn path(self) -> &'static str {
        match self {
            MotionBgsCategory::All => "",
            MotionBgsCategory::Anime => "tag:anime",
            MotionBgsCategory::Games => "tag:games",
            MotionBgsCategory::Ultra4k => "4k",
            MotionBgsCategory::Superhero => "tag:superhero",
            MotionBgsCategory::Nature => "tag:nature",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::all()
            .into_iter()
            .find(|category| category.id().eq_ignore_ascii_case(id.trim()))
    }
}

pub fn search(
    query: &str,
    page: u32,
    category: MotionBgsCategory,
    force: bool,
) -> std::result::Result<Vec<RemoteItem>, SearchError> {
    if !force {
        if let Some(items) = load_cached(query, page, category) {
            log::debug!(
                "MotionBGS cache hit ({} cards, query={query:?}, page={page}, {})",
                items.len(),
                category.id()
            );
            return Ok(items);
        }
    }

    let url = build_url(query, page, category);
    let client = super::http_client()
        .map_err(|err| SearchError::Other(format!("{err:#}")))?;
    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .map_err(request_error)?;

    let status = response.status();
    let status_code = status.as_u16();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let cf_mitigated = response
        .headers()
        .get("cf-mitigated")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let cf_ray = response
        .headers()
        .get("cf-ray")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let body = response
        .text()
        .map_err(|err| SearchError::Offline(format!("reading response: {err}")))?;

    if status_code == 429 {
        log::warn!(
            "MotionBGS rate limit (cf-ray={:?}, retry-after={:?})",
            cf_ray,
            retry_after
        );
        return Err(SearchError::RateLimited { retry_after });
    }

    if status_code == 403 {
        let detail = block_detail(status_code, cf_ray.as_deref(), cf_mitigated.as_deref());
        log::warn!("MotionBGS blocked: {detail}");
        return Err(SearchError::Blocked {
            status: status_code,
            detail,
        });
    }

    if !status.is_success() {
        if is_block_body(&body) {
            let detail = block_detail(
                status_code,
                cf_ray.as_deref(),
                cf_mitigated.as_deref(),
            );
            log::warn!("MotionBGS challenge/block: {detail}");
            return Err(SearchError::Blocked {
                status: status_code,
                detail,
            });
        }
        return Err(SearchError::Other(format!("MotionBGS HTTP {status}")));
    }

    let items = parse_html(&body);
    if items.is_empty() {
        // A successful response can still be a Cloudflare challenge page.
        if is_block_body(&body) {
            let detail = block_detail(
                status_code,
                cf_ray.as_deref(),
                cf_mitigated.as_deref(),
            );
            log::warn!("MotionBGS challenge page: {detail}");
            return Err(SearchError::Blocked {
                status: status_code,
                detail,
            });
        }
        return Err(SearchError::Empty(
            "MotionBGS returned no wallpapers for this search".into(),
        ));
    }

    store_cached(query, page, category, &items);
    Ok(items)
}

fn request_error(err: reqwest::Error) -> SearchError {
    if err.is_timeout() {
        SearchError::Offline("the request timed out".into())
    } else if err.is_connect() {
        SearchError::Offline(format!("connection failed: {err}"))
    } else {
        SearchError::Other(format!("{err}"))
    }
}

fn block_detail(status: u16, cf_ray: Option<&str>, mitigated: Option<&str>) -> String {
    let mut detail = format!("HTTP {status}");
    if let Some(ray) = cf_ray {
        detail.push_str(&format!(", cf-ray {ray}"));
    }
    if let Some(mitigated) = mitigated {
        detail.push_str(&format!(", cf-mitigated {mitigated}"));
    }
    detail
}

/// Markers used by Cloudflare's block/challenge pages. Avoid matching the
/// bare word "cloudflare" because normal pages can reference its scripts.
fn is_block_body(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    [
        "just a moment",
        "cf-chl",
        "attention required",
        "access denied",
        "error 1020",
        "error 1015",
        "error 1010",
        "cf-mitigated",
        "turnstile",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

// ---------------------------------------------------------------------------
// Result cache: successful MotionBGS pages are reused for a while. This makes
// category browsing instant, avoids hammering a Cloudflare-protected site and
// means a temporary block does not empty an already loaded page.
// ---------------------------------------------------------------------------

const CACHE_VERSION: &str = "v1";
const CACHE_TTL_MS: u128 = 20 * 60 * 1000;
const MAX_CACHE_FILES: usize = 120;
const MAX_MEMORY_ENTRIES: usize = 160;

#[derive(Serialize, Deserialize)]
struct CachedResults {
    saved_ms: u128,
    items: Vec<RemoteItem>,
}

static MEMORY_CACHE: OnceLock<Mutex<HashMap<String, (u128, Vec<RemoteItem>)>>> =
    OnceLock::new();
static CACHE_STORES: AtomicUsize = AtomicUsize::new(0);

fn memory_cache() -> &'static Mutex<HashMap<String, (u128, Vec<RemoteItem>)>> {
    MEMORY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember(key: String, saved_ms: u128, items: Vec<RemoteItem>) {
    let mut cache = memory_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    cache.insert(key, (saved_ms, items));
    while cache.len() > MAX_MEMORY_ENTRIES {
        let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, (saved, _))| *saved)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        cache.remove(&oldest);
    }
}

fn cache_key(query: &str, page: u32, category: MotionBgsCategory) -> String {
    let seed = format!(
        "{CACHE_VERSION}|{}|{}|{}",
        query.trim().to_lowercase(),
        page.max(1),
        category.id()
    );
    crate::util::cache_key_for_text(&seed)
}

fn cache_dir() -> PathBuf {
    crate::util::cache_root().join("online")
}

fn cache_path(key: &str) -> PathBuf {
    cache_dir().join(format!("motionbgs-{key}.json"))
}

fn load_cached(
    query: &str,
    page: u32,
    category: MotionBgsCategory,
) -> Option<Vec<RemoteItem>> {
    let key = cache_key(query, page, category);
    let now = crate::util::now_ms();

    {
        let mut cache = memory_cache()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some((saved, items)) = cache.get(&key) {
            if now.saturating_sub(*saved) <= CACHE_TTL_MS {
                return Some(items.clone());
            }
            cache.remove(&key);
        }
    }

    let path = cache_path(&key);
    let raw = std::fs::read_to_string(&path).ok()?;
    let cached: CachedResults = serde_json::from_str(&raw).ok()?;
    if now.saturating_sub(cached.saved_ms) > CACHE_TTL_MS {
        let _ = std::fs::remove_file(&path);
        return None;
    }

    remember(key, cached.saved_ms, cached.items.clone());
    Some(cached.items)
}

fn store_cached(
    query: &str,
    page: u32,
    category: MotionBgsCategory,
    items: &[RemoteItem],
) {
    let key = cache_key(query, page, category);
    let saved_ms = crate::util::now_ms();
    remember(key.clone(), saved_ms, items.to_vec());

    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let entry = CachedResults {
        saved_ms,
        items: items.to_vec(),
    };
    let Ok(json) = serde_json::to_vec(&entry) else {
        return;
    };
    let path = cache_path(&key);
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, json).is_ok() {
        let _ = std::fs::remove_file(&path);
        if std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }

    if CACHE_STORES.fetch_add(1, Ordering::Relaxed) % 20 == 0 {
        prune_cache(&dir);
    }
}

/// Keeps the on-disk cache bounded. Runs only occasionally, on the same
/// background search thread, so it never touches the UI thread.
fn prune_cache(dir: &PathBuf) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = crate::util::now_ms();
    let mut files: Vec<(PathBuf, u128)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("motionbgs-") || !name.ends_with(".json") {
            continue;
        }
        let modified = crate::util::modified_ms(&path);
        if now.saturating_sub(modified) > CACHE_TTL_MS * 6 {
            let _ = std::fs::remove_file(&path);
        } else {
            files.push((path, modified));
        }
    }
    if files.len() > MAX_CACHE_FILES {
        files.sort_by_key(|(_, modified)| *modified);
        let remove_count = files.len() - MAX_CACHE_FILES;
        for (path, _) in files.into_iter().take(remove_count) {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn build_url(query: &str, page: u32, category: MotionBgsCategory) -> String {
    let query = query.trim();
    if !query.is_empty() {
        let encoded = urlencoding::encode(query);
        return if page <= 1 {
            format!("{BASE}/search?q={encoded}")
        } else {
            format!("{BASE}/search?q={encoded}&page={page}")
        };
    }

    let path = category.path();
    if path.is_empty() {
        if page <= 1 {
            format!("{BASE}/")
        } else {
            format!("{BASE}/{page}/")
        }
    } else if page <= 1 {
        format!("{BASE}/{path}/")
    } else {
        format!("{BASE}/{path}/{page}/")
    }
}

/// Parses the `div class=tmb` cards from a MotionBGS page.
pub fn parse_html(html: &str) -> Vec<RemoteItem> {
    let body_start = html.find("<body").unwrap_or(0);
    let body = &html[body_start..];
    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut pos = 0usize;

    // MotionBGS puts many `<a>` cards inside a single `div class=tmb`, so we
    // iterate links and keep those that carry a card-sized thumbnail.
    while let Some(relative) = body[pos..].find("<a ") {
        let start = pos + relative;
        let after_start = start + 3;
        let end = body[after_start..]
            .find("</a>")
            .map(|next| after_start + next)
            .unwrap_or_else(|| (start + 5000).min(body.len()));
        if end <= start {
            break;
        }
        let block = &body[start..end];
        pos = end;

        let is_card = block.contains("/i/c/364x205/media/")
            || block.contains("/i/c/546x308/media/");
        if !is_card {
            continue;
        }

        let Some((id, filename)) = extract_media(block) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }

        let href = extract_href(block).unwrap_or_default();
        let raw_title = extract_title(block).unwrap_or_default();
        let mut title = clean_title(&raw_title);
        if title.is_empty() {
            title = title_from_slug(&filename);
        }
        if title.is_empty() {
            title = format!("MotionBGS {id}");
        }

        let is_4k = block.contains("3840x2160")
            || block.contains("> 4K")
            || block.contains(">4K")
            || filename.to_lowercase().contains("4k");

        let thumb_url = format!("{BASE}/i/c/364x205/media/{id}/{filename}.jpg");
        let thumb_key = crate::util::cache_key_for_text(&thumb_url);
        items.push(RemoteItem {
            id: id.clone(),
            title,
            thumb_url,
            thumb_key,
            full_url: if is_4k {
                format!("{BASE}/dl/4k/{id}/")
            } else {
                format!("{BASE}/dl/hd/{id}/")
            },
            page_url: if href.is_empty() {
                format!("{BASE}/media/{id}/")
            } else {
                format!("{BASE}{href}")
            },
            kind: MediaKind::Video,
            source: "MotionBGS".into(),
            resolution: if is_4k {
                "4K UHD".into()
            } else {
                "1080p HD".into()
            },
        });
    }

    items
}

fn extract_media(block: &str) -> Option<(String, String)> {
    let idx = block
        .find("/i/c/364x205/media/")
        .or_else(|| block.find("/i/c/546x308/media/"))
        .or_else(|| block.find("/media/"))?;
    let after = &block[idx..];
    let media = after.split("/media/").nth(1)?;
    let mut parts = media.splitn(2, '/');
    let id = parts.next()?.trim();
    let rest = parts.next()?;
    let jpg = rest.find(".jpg")?;
    let filename = &rest[..jpg];
    if id.is_empty()
        || filename.is_empty()
        || !id.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some((id.to_string(), filename.to_string()))
}

fn extract_title(block: &str) -> Option<String> {
    let start = block.find("title=\"")? + 7;
    let end = block[start..].find('"')? + start;
    Some(block[start..end].to_string())
}

fn extract_href(block: &str) -> Option<String> {
    let start = block.find("href=")? + 5;
    let rest = &block[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(rest.len());
    let value = rest[..end].trim_matches('"').trim_matches('\'');
    if value.starts_with('/') {
        Some(value.to_string())
    } else {
        None
    }
}

fn clean_title(raw: &str) -> String {
    let decoded = html_decode(raw.trim());
    let lower = decoded.to_lowercase();
    for suffix in [" live wallpaper", " animated wallpaper", " wallpaper"] {
        if lower.ends_with(suffix) {
            return decoded[..decoded.len() - suffix.len()].trim().to_string();
        }
    }
    decoded
}

fn title_from_slug(filename: &str) -> String {
    let base = filename.split('.').next().unwrap_or(filename);
    let words: Vec<String> = base
        .split('-')
        .filter(|word| !word.is_empty() && !word.chars().all(|c| c.is_ascii_digit()))
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    words.join(" ")
}

fn html_decode(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    <html><head><meta content=https://motionbgs.com/media/1/ignored.jpg property=og:image></head>
    <body>
    <div class=tmb><a title="Itachi Uchiha in Front of the Red Moon animated wallpaper" href=/itachi-red-moon>
      <figure><picture>
        <source srcset=/i/c/546x308/media/1057/itachi-red-moon.jpg>
        <img src=/i/c/364x205/media/1057/itachi-red-moon.jpg width=364>
      </picture></figure>
      <span class=ttl>Itachi Uchiha in Front of the Red Moon</span><span class=frm> 4K </span>
    </a>
    <a title="Naruto Halloween Party live wallpaper" href=/naruto-halloween>
      <figure><picture><img src=/i/c/364x205/media/10216/narutos-halloween-party.jpg width=364></picture></figure>
      <span class=ttl>Naruto Halloween Party</span><span class=frm> 1080p </span>
    </a></div>
    </body></html>
    "#;

    #[test]
    fn builds_category_urls() {
        assert_eq!(
            build_url("", 1, MotionBgsCategory::All),
            "https://motionbgs.com/"
        );
        assert_eq!(
            build_url("", 2, MotionBgsCategory::Anime),
            "https://motionbgs.com/tag:anime/2/"
        );
        assert_eq!(
            build_url("", 1, MotionBgsCategory::Ultra4k),
            "https://motionbgs.com/4k/"
        );
        assert_eq!(
            build_url("gojo", 1, MotionBgsCategory::Anime),
            "https://motionbgs.com/search?q=gojo"
        );
        assert_eq!(MotionBgsCategory::from_id("4k"), Some(MotionBgsCategory::Ultra4k));
        assert_eq!(MotionBgsCategory::from_id("nope"), None);
    }

    #[test]
    fn detects_cloudflare_block_pages() {
        assert!(is_block_body("Just a moment..."));
        assert!(is_block_body("<title>Access denied | motionbgs.com used Cloudflare</title>"));
        assert!(is_block_body("Error 1020: Access denied"));
        assert!(!is_block_body("<html><body><div class=tmb>normal catalog</div></body></html>"));
    }

    #[test]
    fn cache_key_separates_queries_and_pages() {
        let a = cache_key("gojo", 1, MotionBgsCategory::All);
        let b = cache_key("gojo", 2, MotionBgsCategory::All);
        let c = cache_key("naruto", 1, MotionBgsCategory::All);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, cache_key(" GOJO ", 1, MotionBgsCategory::All));
    }

    #[test]
    fn parses_cards() {
        let items = parse_html(SAMPLE);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "1057");
        assert_eq!(items[0].title, "Itachi Uchiha in Front of the Red Moon");
        assert_eq!(items[0].resolution, "4K UHD");
        assert_eq!(items[0].full_url, "https://motionbgs.com/dl/4k/1057/");
        assert_eq!(
            items[0].thumb_url,
            "https://motionbgs.com/i/c/364x205/media/1057/itachi-red-moon.jpg"
        );
        assert_eq!(items[1].id, "10216");
        assert_eq!(items[1].full_url, "https://motionbgs.com/dl/hd/10216/");
        assert_eq!(items[1].kind, MediaKind::Video);
    }
}
