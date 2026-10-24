//! MotionBGS catalog (https://motionbgs.com).
//!
//! The site has no public JSON API, so the catalog is scraped from the same
//! HTML cards used by the browser. This mirrors the approach of the original
//! Linux Aura client.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use super::RemoteItem;
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
) -> Result<Vec<RemoteItem>> {
    let url = build_url(query, page, category);

    let html = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(25))
        .build()
        .context("build motionbgs client")?
        .get(&url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .context("request motionbgs")?
        .error_for_status()
        .context("motionbgs HTTP error")?
        .text()
        .context("read motionbgs response")?;

    let items = parse_html(&html);
    if items.is_empty() {
        return Err(anyhow!("MotionBGS returned no wallpapers for this query"));
    }
    Ok(items)
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

        items.push(RemoteItem {
            id: id.clone(),
            title,
            thumb_url: format!("{BASE}/i/c/364x205/media/{id}/{filename}.jpg"),
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
