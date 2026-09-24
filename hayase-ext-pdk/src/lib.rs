//! Shared helpers for Hayase torrent extensions written in Rust.
//!
//! Same worker contract as original Hayase: `test`, `single`, `batch`, `movie`.
//! Catalogs are the same JSON (`index.json` + `code` URL). `code` is the compiled
//! module the app downloads, equivalent to original `.js` workers.

use extism_pdk::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

pub mod providers;

pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

const TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "http://nyaa.tracker.wf:7777/announce",
];

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    #[serde(alias = "anilist_id", alias = "anilistId")]
    pub anilist_id: i32,
    pub titles: Vec<String>,
    pub episode: i32,
    #[serde(alias = "episode_count")]
    pub episode_count: Option<i32>,
    pub resolution: String,
    pub exclusions: Vec<String>,
    #[serde(alias = "anidb_aid")]
    pub anidb_aid: Option<i32>,
    #[serde(alias = "anidb_eid")]
    pub anidb_eid: Option<i32>,
    #[serde(alias = "episode_candidates")]
    pub episode_candidates: Vec<i32>,
    #[serde(alias = "tvdb_id")]
    pub tvdb_id: Option<i32>,
    #[serde(alias = "tvdbEId", alias = "tvdb_eid")]
    pub tvdb_eid: Option<i32>,
    #[serde(alias = "tmdb_id")]
    pub tmdb_id: Option<String>,
    #[serde(alias = "mal_id")]
    pub mal_id: Option<i32>,
    #[serde(alias = "is_movie")]
    pub is_movie: bool,
    #[serde(alias = "is_single")]
    pub is_single: bool,
    #[serde(alias = "after_iso", alias = "after")]
    pub after_iso: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchInput {
    #[serde(flatten)]
    pub query: Query,
    pub options: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub title: String,
    pub link: String,
    pub seeders: i32,
    pub leechers: i32,
    pub downloads: i32,
    pub accuracy: String,
    pub hash: String,
    pub size: u64,
    pub date: String,
    pub source: String,
    pub resolution: Option<String>,
    pub episode: Option<String>,
    pub group: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedName {
    pub episode: Option<String>,
    pub resolution: Option<String>,
    pub group: Option<String>,
}

pub fn option_bool(input: &SearchInput, key: &str, default: bool) -> bool {
    input
        .options
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

pub fn http_get(url: &str) -> Result<String, String> {
    let req = HttpRequest::new(url)
        .with_method("GET")
        .with_header("user-agent", USER_AGENT)
        .with_header("accept", "application/xml, text/xml, application/json, text/html, */*");
    let res = http::request::<()>(&req, None).map_err(|e| e.to_string())?;
    let status = res.status_code();
    let body = String::from_utf8_lossy(&res.body()).into_owned();
    if status >= 500 {
        return Err(format!("HTTP {status}"));
    }
    Ok(body)
}

pub fn http_ok(url: &str) -> bool {
    http_get(url).is_ok()
}

pub fn encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn magnet_hash(magnet_or_link: &str) -> Option<String> {
    let lower = magnet_or_link.to_ascii_lowercase();
    let idx = lower.find("btih:")?;
    let rest = &magnet_or_link[idx + 5..];
    let hash: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(hash.to_ascii_lowercase());
    }
    if hash.len() == 32 {
        return Some(hash.to_ascii_lowercase());
    }
    None
}

pub fn parse_size(raw: &str) -> u64 {
    let s = raw.trim().replace(',', "");
    let mut num = String::new();
    let mut unit = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else if c.is_ascii_alphabetic() {
            unit.push(c);
        }
    }
    let n: f64 = num.parse().unwrap_or(0.0);
    let mul = match unit.to_ascii_lowercase().as_str() {
        "kib" | "kb" | "k" => 1024.0,
        "mib" | "mb" | "m" => 1024.0 * 1024.0,
        "gib" | "gb" | "g" => 1024.0 * 1024.0 * 1024.0,
        "tib" | "tb" | "t" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (n * mul) as u64
}

pub fn guess_resolution(title: &str) -> Option<String> {
    let t = title.to_ascii_lowercase();
    for res in ["2160", "1080", "720", "480"] {
        if t.contains(res) {
            return Some(res.into());
        }
    }
    None
}

pub fn parse_filename(name: &str) -> ParsedName {
    static RES: OnceLock<Regex> = OnceLock::new();
    static EP: OnceLock<Regex> = OnceLock::new();
    static GROUP: OnceLock<Regex> = OnceLock::new();
    let res_re = RES.get_or_init(|| Regex::new(r"(?i)\b(2160|1080|720|480)p?\b").unwrap());
    let ep_re = EP.get_or_init(|| {
        Regex::new(r"(?i)(?:\s|\[|\(|\.)(?:e|ep|episode)?[\s._-]*(\d{1,4})(?:v\d+)?(?:\s|\]|\)|\.|$)")
            .unwrap()
    });
    let group_re = GROUP.get_or_init(|| Regex::new(r"^\[([^\]]+)\]").unwrap());
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    ParsedName {
        resolution: res_re.captures(stem).map(|c| c[1].to_string()),
        episode: ep_re.captures(stem).map(|c| c[1].to_string()),
        group: group_re.captures(stem).map(|c| c[1].to_string()),
    }
}

pub fn build_magnet(hash: &str, name: &str) -> String {
    let mut magnet = format!("magnet:?xt=urn:btih:{}", hash.to_ascii_lowercase());
    if !name.is_empty() {
        magnet.push_str("&dn=");
        magnet.push_str(&encode_query(name));
    }
    for tr in TRACKERS {
        magnet.push_str("&tr=");
        magnet.push_str(&encode_query(tr));
    }
    magnet
}

pub fn is_http_torrent_url(link: &str) -> bool {
    let lower = link.trim().to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && (lower.contains(".torrent")
            || lower.contains("/download/")
            || lower.contains("/storage/torrent/"))
}

pub fn nyaa_download_url(link: &str) -> Option<String> {
    let lower = link.to_ascii_lowercase();
    let host = if lower.contains("sukebei.nyaa.si") {
        "https://sukebei.nyaa.si"
    } else if lower.contains("nyaa.si") {
        "https://nyaa.si"
    } else if lower.contains("nyaa.land") {
        "https://nyaa.land"
    } else {
        return None;
    };
    if let Some(rest) = lower.split("/download/").nth(1) {
        let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !id.is_empty() {
            return Some(format!("{host}/download/{id}.torrent"));
        }
    }
    if let Some(rest) = lower.split("/view/").nth(1) {
        let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !id.is_empty() {
            return Some(format!("{host}/download/{id}.torrent"));
        }
    }
    None
}

pub fn prefer_play_link(link: &str, hash: &str, title: &str) -> String {
    let link = link.trim();
    if let Some(url) = nyaa_download_url(link) {
        return url;
    }
    if is_http_torrent_url(link) || link.starts_with("magnet:") {
        link.to_string()
    } else {
        build_magnet(hash, title)
    }
}

pub fn torrent_from_parts(
    title: String,
    hash: String,
    source: &str,
    accuracy: &str,
    seeders: i32,
    leechers: i32,
    downloads: i32,
    size: u64,
    date: String,
) -> Hit {
    let parsed = parse_filename(&title);
    let link = if hash.is_empty() {
        String::new()
    } else {
        build_magnet(&hash, &title)
    };
    let kind = if title.to_ascii_lowercase().contains("batch") {
        Some("batch".into())
    } else {
        None
    };
    Hit {
        resolution: parsed.resolution.or_else(|| guess_resolution(&title)),
        episode: parsed.episode,
        group: parsed.group,
        title,
        link,
        seeders,
        leechers,
        downloads,
        accuracy: accuracy.into(),
        hash,
        size,
        date,
        source: source.into(),
        kind,
    }
}

pub fn decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .trim()
        .to_string()
}

pub fn trim_title_for_query(title: &str) -> String {
    let raw = title.trim();
    if raw.is_empty() {
        return String::new();
    }
    let base = raw
        .split_once(':')
        .filter(|(h, _)| !h.is_empty())
        .map(|(h, _)| h)
        .unwrap_or(raw);
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .take(4)
        .collect();
    if !words.is_empty() {
        return words.join(" ");
    }
    let fallback: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    fallback
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn query_variants_for(query: &Query) -> Vec<String> {
    let mut episodes = Vec::new();
    if query.episode > 0 {
        episodes.push(query.episode);
    }
    for ep in &query.episode_candidates {
        if *ep > 0 && !episodes.contains(ep) {
            episodes.push(*ep);
        }
    }
    query_variants_eps(&query.titles, &episodes)
}

fn query_variants_eps(titles: &[String], episodes: &[i32]) -> Vec<String> {
    let mut bases = Vec::new();
    let mut seen_base = HashSet::new();
    for title in titles {
        let q = trim_title_for_query(title);
        if q.is_empty() {
            continue;
        }
        let key = q.to_ascii_lowercase();
        if !seen_base.insert(key) {
            continue;
        }
        bases.push(q);
        if bases.len() >= 2 {
            break;
        }
    }
    let mut out = Vec::new();
    let mut seen_q = HashSet::new();
    let mut push = |out: &mut Vec<String>, q: String| {
        if seen_q.insert(q.to_ascii_lowercase()) {
            out.push(q);
        }
    };
    for base in &bases {
        for ep in episodes {
            push(&mut out, format!("{base} {ep:02}"));
            if out.len() >= 4 {
                return out;
            }
        }
    }
    for base in bases {
        push(&mut out, base);
        if out.len() >= 4 {
            break;
        }
    }
    out
}

pub fn parse_nyaa_rss(xml: &str, source: &str, accuracy: &str) -> Vec<Hit> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static HASH: OnceLock<Regex> = OnceLock::new();
    static SEED: OnceLock<Regex> = OnceLock::new();
    static LEECH: OnceLock<Regex> = OnceLock::new();
    static DOWN: OnceLock<Regex> = OnceLock::new();
    static SIZE: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    static GUID: OnceLock<Regex> = OnceLock::new();
    static ENCLOSURE: OnceLock<Regex> = OnceLock::new();

    let item_re = ITEM.get_or_init(|| Regex::new(r"(?s)<item>(.*?)</item>").unwrap());
    let title_re = TITLE.get_or_init(|| {
        Regex::new(r"(?s)<title>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</title>").unwrap()
    });
    let link_re = LINK.get_or_init(|| Regex::new(r"(?s)<link>(.*?)</link>").unwrap());
    let hash_re =
        HASH.get_or_init(|| Regex::new(r"(?s)<nyaa:infoHash>(.*?)</nyaa:infoHash>").unwrap());
    let seed_re =
        SEED.get_or_init(|| Regex::new(r"(?s)<nyaa:seeders>(.*?)</nyaa:seeders>").unwrap());
    let leech_re =
        LEECH.get_or_init(|| Regex::new(r"(?s)<nyaa:leechers>(.*?)</nyaa:leechers>").unwrap());
    let down_re =
        DOWN.get_or_init(|| Regex::new(r"(?s)<nyaa:downloads>(.*?)</nyaa:downloads>").unwrap());
    let size_re = SIZE.get_or_init(|| Regex::new(r"(?s)<nyaa:size>(.*?)</nyaa:size>").unwrap());
    let date_re = DATE.get_or_init(|| Regex::new(r"(?s)<pubDate>(.*?)</pubDate>").unwrap());
    let guid_re = GUID.get_or_init(|| Regex::new(r"(?s)<guid[^>]*>(.*?)</guid>").unwrap());
    let enclosure_re = ENCLOSURE.get_or_init(|| {
        Regex::new(r#"(?is)<enclosure[^>]*url=["']([^"']+)["']"#).unwrap()
    });

    let mut out = vec![];
    for cap in item_re.captures_iter(xml) {
        let item = &cap[1];
        let title = title_re
            .captures(item)
            .map(|c| decode(&c[1]))
            .unwrap_or_default();
        let link = enclosure_re
            .captures(item)
            .map(|c| decode(&c[1]))
            .or_else(|| link_re.captures(item).map(|c| decode(&c[1])))
            .or_else(|| guid_re.captures(item).map(|c| decode(&c[1])))
            .unwrap_or_default();
        let hash = hash_re
            .captures(item)
            .map(|c| c[1].trim().to_ascii_lowercase())
            .or_else(|| magnet_hash(&link))
            .unwrap_or_default();
        if title.is_empty() || hash.is_empty() {
            continue;
        }
        let parsed = parse_filename(&title);
        out.push(Hit {
            seeders: seed_re
                .captures(item)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(0),
            leechers: leech_re
                .captures(item)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(0),
            downloads: down_re
                .captures(item)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(0),
            size: size_re
                .captures(item)
                .map(|c| parse_size(&c[1]))
                .unwrap_or(0),
            date: date_re
                .captures(item)
                .map(|c| c[1].to_string())
                .unwrap_or_default(),
            link: prefer_play_link(&link, &hash, &title),
            resolution: parsed.resolution.or_else(|| guess_resolution(&title)),
            episode: parsed.episode,
            group: parsed.group,
            accuracy: accuracy.into(),
            source: source.into(),
            kind: if title.to_ascii_lowercase().contains("batch") {
                Some("batch".into())
            } else {
                None
            },
            hash,
            title,
        });
    }
    out
}

pub fn search_nyaa_queries(
    queries: Vec<String>,
    make_url: impl Fn(&str) -> String,
    source: &str,
    accuracy: &str,
) -> Vec<Hit> {
    let mut last = Vec::new();
    for q in queries {
        let url = make_url(&q);
        if let Ok(body) = http_get(&url) {
            last = parse_nyaa_rss(&body, source, accuracy);
            if !last.is_empty() {
                return last;
            }
        }
    }
    last
}

pub fn json_get(url: &str) -> Result<Value, String> {
    let body = http_get(url)?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Original Hayase worker exports: `single`, `batch`, `movie`, `test`.
#[macro_export]
macro_rules! torrent_plugin {
    ($search:expr, $test:expr) => {
        fn __hayase_run(
            input: &$crate::SearchInput,
        ) -> Result<Vec<$crate::Hit>, String> {
            $search(input)
        }

        #[extism_pdk::plugin_fn]
        pub fn single(
            extism_pdk::Json(input): extism_pdk::Json<$crate::SearchInput>,
        ) -> extism_pdk::FnResult<extism_pdk::Json<Vec<$crate::Hit>>> {
            match __hayase_run(&input) {
                Ok(rows) => Ok(extism_pdk::Json(rows)),
                Err(err) => Err(extism_pdk::WithReturnCode::new(
                    extism_pdk::Error::msg(err),
                    1,
                )),
            }
        }

        #[extism_pdk::plugin_fn]
        pub fn batch(
            extism_pdk::Json(input): extism_pdk::Json<$crate::SearchInput>,
        ) -> extism_pdk::FnResult<extism_pdk::Json<Vec<$crate::Hit>>> {
            match __hayase_run(&input) {
                Ok(rows) => Ok(extism_pdk::Json(rows)),
                Err(err) => Err(extism_pdk::WithReturnCode::new(
                    extism_pdk::Error::msg(err),
                    1,
                )),
            }
        }

        #[extism_pdk::plugin_fn]
        pub fn movie(
            extism_pdk::Json(input): extism_pdk::Json<$crate::SearchInput>,
        ) -> extism_pdk::FnResult<extism_pdk::Json<Vec<$crate::Hit>>> {
            match __hayase_run(&input) {
                Ok(rows) => Ok(extism_pdk::Json(rows)),
                Err(err) => Err(extism_pdk::WithReturnCode::new(
                    extism_pdk::Error::msg(err),
                    1,
                )),
            }
        }

        #[extism_pdk::plugin_fn]
        pub fn search(
            extism_pdk::Json(input): extism_pdk::Json<$crate::SearchInput>,
        ) -> extism_pdk::FnResult<extism_pdk::Json<Vec<$crate::Hit>>> {
            match __hayase_run(&input) {
                Ok(rows) => Ok(extism_pdk::Json(rows)),
                Err(err) => Err(extism_pdk::WithReturnCode::new(
                    extism_pdk::Error::msg(err),
                    1,
                )),
            }
        }

        #[extism_pdk::plugin_fn]
        pub fn test(_: ()) -> extism_pdk::FnResult<extism_pdk::Json<bool>> {
            Ok(extism_pdk::Json($test()))
        }
    };
}
