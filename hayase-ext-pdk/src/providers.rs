use crate::{
    decode, encode_query, http_get, http_ok, json_get, magnet_hash, option_bool, parse_size,
    query_variants_for, search_nyaa_queries, torrent_from_parts, Hit, Query, SearchInput,
};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

pub fn nyaa_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    Ok(nyaa_like(
        &input.query,
        "https://nyaa.si",
        "1_2",
        "nyaa",
        "medium",
        None,
        None,
    ))
}

pub fn nyaa_test() -> bool {
    http_ok("https://nyaa.si")
}

pub fn sukebei_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    Ok(nyaa_like(
        &input.query,
        "https://sukebei.nyaa.si",
        "1_1",
        "sukebei",
        "medium",
        None,
        None,
    ))
}

pub fn sukebei_test() -> bool {
    http_ok("https://sukebei.nyaa.si")
}

pub fn yameii_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    Ok(nyaa_like(
        &input.query,
        "https://nyaa.si",
        "1_2",
        "yameii",
        "high",
        Some("Yameii"),
        None,
    ))
}

pub fn yameii_test() -> bool {
    http_ok("https://nyaa.si/?u=Yameii")
}

pub fn toonshub_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    let rows = nyaa_like(
        &input.query,
        "https://nyaa.si",
        "1_2",
        "toonshub",
        "high",
        None,
        Some("[ToonsHub]"),
    );
    Ok(rows
        .into_iter()
        .filter(|r| r.title.contains("[ToonsHub]"))
        .collect())
}

pub fn toonshub_test() -> bool {
    http_ok("https://nyaa.si")
}

fn nyaa_like(
    query: &Query,
    host: &str,
    category: &str,
    source: &str,
    accuracy: &str,
    user: Option<&str>,
    prefix: Option<&str>,
) -> Vec<Hit> {
    let queries = query_variants_for(query)
        .into_iter()
        .map(|q| match prefix {
            Some(p) => format!("{p} {q}"),
            None => q,
        })
        .collect();
    let user_q = user
        .map(|u| format!("u={u}&"))
        .unwrap_or_default();
    search_nyaa_queries(
        queries,
        |q| {
            format!(
                "{host}/?{user_q}page=rss&c={category}&f=0&s=id&o=desc&q={}",
                encode_query(q)
            )
        },
        source,
        accuracy,
    )
}

pub fn seadex_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    let query = &input.query;
    if query.anilist_id <= 0 {
        return Ok(vec![]);
    }
    let url = format!(
        "https://releases.moe/api/collections/entries/records?expand=trs&perPage=10&filter={}",
        encode_query(&format!("(alID={})", query.anilist_id))
    );
    let data: Records = serde_json::from_str(&http_get(&url)?).unwrap_or_default();
    let show = query
        .titles
        .first()
        .cloned()
        .unwrap_or_else(|| "release".into());
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in data.items {
        let Some(trs) = item.expand.and_then(|e| e.trs) else {
            continue;
        };
        for tr in trs {
            let hash = tr.info_hash.unwrap_or_default().trim().to_ascii_lowercase();
            if hash.len() < 32 || !seen.insert(hash.clone()) {
                continue;
            }
            let files = tr.files.unwrap_or_default();
            let size: u64 = files.iter().filter_map(|f| f.length).sum();
            let first_name = files.iter().find_map(|f| f.name.clone()).unwrap_or_default();
            let group = tr.release_group.as_deref().unwrap_or("");
            let dual = if tr.dual_audio.unwrap_or(false) {
                " Dual Audio"
            } else {
                ""
            };
            let title = if files.len() == 1 && !first_name.is_empty() {
                first_name
            } else if !group.is_empty() {
                format!("[{group}] {show}{dual}")
            } else {
                format!("{show}{dual}")
            };
            let accuracy = if tr.is_best.unwrap_or(false) {
                "high"
            } else {
                "medium"
            };
            let mut row = torrent_from_parts(
                title,
                hash,
                "seadex",
                accuracy,
                0,
                0,
                0,
                size,
                tr.created.unwrap_or_default(),
            );
            row.kind = if tr.is_best.unwrap_or(false) {
                Some("best".into())
            } else if files.len() > 1 {
                Some("batch".into())
            } else {
                None
            };
            out.push(row);
        }
    }
    Ok(out)
}

pub fn seadex_test() -> bool {
    http_ok("https://releases.moe/api/collections/entries/records")
}

#[derive(Debug, Deserialize, Default)]
struct Records {
    #[serde(default)]
    items: Vec<Entry>,
}

#[derive(Debug, Deserialize, Default)]
struct Entry {
    #[serde(default)]
    expand: Option<Expand>,
}

#[derive(Debug, Deserialize, Default)]
struct Expand {
    #[serde(default)]
    trs: Option<Vec<SdTorrent>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SdTorrent {
    info_hash: Option<String>,
    release_group: Option<String>,
    dual_audio: Option<bool>,
    is_best: Option<bool>,
    created: Option<String>,
    files: Option<Vec<SdFile>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct SdFile {
    name: Option<String>,
    length: Option<u64>,
}

pub fn animetosho_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    const BASE: &str = "https://feed.animetosho.org/json";
    let query = &input.query;
    let use_torrent = option_bool(input, "useTorrent", false);
    let mut rows: Vec<ToshoRow> = Vec::new();
    let mut accuracy = "medium";
    if let Some(eid) = query.anidb_eid.filter(|id| *id > 0) {
        rows = fetch_tosho(&format!("{BASE}?eid={eid}"));
        if !rows.is_empty() {
            accuracy = "high";
        }
    }
    if rows.is_empty() {
        if let Some(aid) = query.anidb_aid.filter(|id| *id > 0) {
            rows = fetch_tosho(&format!("{BASE}?aid={aid}"));
            if !rows.is_empty() {
                accuracy = "high";
            }
        }
    }
    if rows.is_empty() {
        for q in query_variants_for(query) {
            rows = fetch_tosho(&format!("{BASE}?q={}", encode_query(&q)));
            if !rows.is_empty() {
                break;
            }
        }
    }
    Ok(rows
        .into_iter()
        .filter_map(|row| row.into_result(accuracy, use_torrent, "animetosho"))
        .collect())
}

pub fn animetosho_test() -> bool {
    http_ok("https://feed.animetosho.org/json")
}

fn fetch_tosho(url: &str) -> Vec<ToshoRow> {
    serde_json::from_str(&http_get(url).unwrap_or_default()).unwrap_or_default()
}

#[derive(Debug, Deserialize, Default)]
struct ToshoRow {
    title: Option<String>,
    torrent_name: Option<String>,
    magnet_uri: Option<String>,
    torrent_url: Option<String>,
    info_hash: Option<String>,
    seeders: Option<i32>,
    leechers: Option<i32>,
    num_complete: Option<i32>,
    torrent_downloaded_count: Option<i32>,
    total_size: Option<u64>,
    size_string: Option<String>,
    timestamp: Option<i64>,
}

impl ToshoRow {
    fn into_result(self, accuracy: &str, use_torrent: bool, source: &str) -> Option<Hit> {
        let title = self.title.or(self.torrent_name).unwrap_or_default();
        let torrent_url = self.torrent_url.filter(|u| crate::is_http_torrent_url(u));
        let magnet = self.magnet_uri.unwrap_or_default();
        let hash = self
            .info_hash
            .or_else(|| magnet_hash(&magnet))
            .or_else(|| torrent_url.as_deref().and_then(magnet_hash))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if title.is_empty() || hash.is_empty() {
            return None;
        }
        let mut row = torrent_from_parts(
            title,
            hash,
            source,
            accuracy,
            self.seeders.unwrap_or(0),
            self.leechers.unwrap_or(0),
            self.num_complete
                .or(self.torrent_downloaded_count)
                .unwrap_or(0),
            self.total_size
                .or_else(|| self.size_string.as_deref().map(parse_size))
                .unwrap_or(0),
            self.timestamp.map(|t| t.to_string()).unwrap_or_default(),
        );
        if use_torrent {
            if let Some(url) = torrent_url {
                row.link = url;
            } else if magnet.starts_with("magnet:") {
                row.link = magnet;
            }
        } else if magnet.starts_with("magnet:") {
            row.link = magnet;
        } else if let Some(url) = torrent_url {
            row.link = url;
        }
        Some(row)
    }
}

pub fn animetosho_new_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    const BASE: &str = "https://feed.animetosho.xyz/json/v1/";
    const QUALITIES: [&str; 4] = ["1080", "720", "540", "480"];
    let query = &input.query;
    let use_torrent = option_bool(input, "useTorrent", false);
    let map = |entries: Vec<NewRow>| {
        let mut excl: Vec<String> = query
            .exclusions
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        if !query.resolution.is_empty() && query.resolution != "0" {
            for q in QUALITIES {
                if q != query.resolution {
                    excl.push(format!("{q}p"));
                }
            }
        }
        entries
            .into_iter()
            .filter_map(|row| {
                let title = row.title.unwrap_or_default();
                let lower = title.to_ascii_lowercase();
                if excl.iter().any(|e| !e.is_empty() && lower.contains(e)) {
                    return None;
                }
                let magnet = row.magnet.unwrap_or_default();
                let torrent_url = row.torrent_url.unwrap_or_default();
                let hash = row
                    .info_hash
                    .or_else(|| magnet_hash(&magnet))
                    .or_else(|| magnet_hash(&torrent_url))
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if title.is_empty() || hash.len() < 32 {
                    return None;
                }
                let mut out = torrent_from_parts(
                    title,
                    hash,
                    "animetosho-new",
                    "medium",
                    clamp_peers(row.seeders),
                    clamp_peers(row.leechers),
                    row.downloads.unwrap_or(0),
                    row.size_bytes.unwrap_or(0),
                    row.date_added.unwrap_or_default(),
                );
                if use_torrent && crate::is_http_torrent_url(&torrent_url) {
                    out.link = torrent_url;
                } else if magnet.starts_with("magnet:") {
                    out.link = magnet;
                }
                Some(out)
            })
            .collect::<Vec<_>>()
    };
    if let Some(eid) = query.anidb_eid.filter(|id| *id > 0) {
        let url = format!("{BASE}episodes/{eid}?limit=100");
        if let Ok(body) = http_get(&url) {
            let wrap: Envelope = serde_json::from_str(&body).unwrap_or_default();
            let rows = wrap.data.and_then(|d| d.releases).unwrap_or_default();
            if !rows.is_empty() {
                return Ok(map(rows));
            }
        }
    }
    if query.is_movie || query.is_single {
        if let Some(aid) = query.anidb_aid.filter(|id| *id > 0) {
            let url = format!("{BASE}series/anidb/{aid}?limit=100");
            if let Ok(body) = http_get(&url) {
                let wrap: Envelope = serde_json::from_str(&body).unwrap_or_default();
                let rows = wrap.data.and_then(|d| d.releases).unwrap_or_default();
                if !rows.is_empty() {
                    return Ok(map(rows));
                }
            }
        }
    }
    Ok(vec![])
}

pub fn animetosho_new_test() -> bool {
    http_ok("https://feed.animetosho.xyz/json/v1/")
}

fn clamp_peers(n: Option<i32>) -> i32 {
    let n = n.unwrap_or(0);
    if n >= 30_000 {
        0
    } else {
        n
    }
}

#[derive(Debug, Deserialize, Default)]
struct Envelope {
    data: Option<Payload>,
}

#[derive(Debug, Deserialize, Default)]
struct Payload {
    releases: Option<Vec<NewRow>>,
}

#[derive(Debug, Deserialize, Default)]
struct NewRow {
    title: Option<String>,
    torrent_url: Option<String>,
    magnet: Option<String>,
    seeders: Option<i32>,
    leechers: Option<i32>,
    downloads: Option<i32>,
    info_hash: Option<String>,
    size_bytes: Option<u64>,
    date_added: Option<String>,
}

pub fn subsplease_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    let query = &input.query;
    let q = crate::trim_title_for_query(&query.titles.first().cloned().unwrap_or_default());
    if q.is_empty() {
        return Ok(vec![]);
    }
    let url = format!(
        "https://subsplease.org/api/?f=search&tz=UTC&s={}",
        encode_query(&q)
    );
    let data: Value = serde_json::from_str(&http_get(&url)?).unwrap_or(Value::Null);
    let Some(obj) = data.as_object() else {
        return Ok(vec![]);
    };
    let mut exact = Vec::new();
    let mut rest = Vec::new();
    for (key, entry) in obj {
        let episode = entry.get("episode").and_then(|v| v.as_str()).unwrap_or("");
        let matches_ep = episode_matches(episode, query);
        let show = entry
            .get("show")
            .and_then(|v| v.as_str())
            .unwrap_or(key.as_str());
        let date = entry
            .get("release_date")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(downloads) = entry.get("downloads").and_then(|v| v.as_array()) else {
            continue;
        };
        for dl in downloads {
            let magnet = dl.get("magnet").and_then(|v| v.as_str()).unwrap_or("");
            let Some(hash) = magnet_hash(magnet) else {
                continue;
            };
            let res = dl.get("res").and_then(|v| v.as_str()).unwrap_or("");
            let title = if res.is_empty() {
                format!("[SubsPlease] {show} - {episode}")
            } else {
                format!("[SubsPlease] {show} - {episode} ({res}p)")
            };
            let size = magnet
                .split(['?', '&'])
                .find_map(|p| p.strip_prefix("xl=").and_then(|n| n.parse().ok()))
                .unwrap_or(0);
            let mut row =
                torrent_from_parts(title, hash, "subsplease", "high", 0, 0, 0, size, date.clone());
            if magnet.starts_with("magnet:") {
                row.link = magnet.to_string();
            }
            if matches_ep {
                exact.push(row);
            } else {
                rest.push(row);
            }
        }
    }
    Ok(if exact.is_empty() { rest } else { exact })
}

pub fn subsplease_test() -> bool {
    http_ok("https://subsplease.org/api/")
}

fn episode_matches(episode: &str, query: &Query) -> bool {
    if query.episode <= 0 {
        return true;
    }
    if let Ok(n) = episode.trim().parse::<i32>() {
        return n == query.episode || query.episode_candidates.contains(&n);
    }
    episode.is_empty() || episode.contains('-') || episode.eq_ignore_ascii_case("batch")
}

pub fn tokyotosho_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    let mut last = Vec::new();
    for q in query_variants_for(&input.query) {
        let url = format!(
            "https://www.tokyotosho.info/rss.php?terms={}",
            encode_query(&q)
        );
        if let Ok(body) = http_get(&url) {
            last = parse_tokyotosho_rss(&body);
            if !last.is_empty() {
                break;
            }
        }
    }
    Ok(last)
}

pub fn tokyotosho_test() -> bool {
    http_ok("https://www.tokyotosho.info")
}

fn parse_tokyotosho_rss(xml: &str) -> Vec<Hit> {
    static ITEM: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static DESC: OnceLock<Regex> = OnceLock::new();
    static MAGNET: OnceLock<Regex> = OnceLock::new();
    static SIZE: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    let item_re = ITEM.get_or_init(|| Regex::new(r"(?s)<item>(.*?)</item>").unwrap());
    let title_re = TITLE.get_or_init(|| {
        Regex::new(r"(?s)<title>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</title>").unwrap()
    });
    let desc_re = DESC.get_or_init(|| {
        Regex::new(r"(?s)<description>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</description>").unwrap()
    });
    let magnet_re =
        MAGNET.get_or_init(|| Regex::new(r"(?i)magnet:\?xt=urn:btih:([a-z0-9]+)").unwrap());
    let size_re = SIZE.get_or_init(|| {
        Regex::new(r"(?i)Size:\s*([\d.]+\s*(?:KiB|MiB|GiB|KB|MB|GB))").unwrap()
    });
    let date_re = DATE.get_or_init(|| Regex::new(r"(?s)<pubDate>(.*?)</pubDate>").unwrap());
    let mut out = vec![];
    for cap in item_re.captures_iter(xml) {
        let item = &cap[1];
        let title = title_re
            .captures(item)
            .map(|c| decode(&c[1]))
            .unwrap_or_default();
        let desc = desc_re
            .captures(item)
            .map(|c| decode(&c[1]))
            .unwrap_or_default();
        let blob = format!("{item} {desc}");
        let Some(hash) = magnet_re
            .captures(&blob)
            .map(|c| c[1].to_ascii_lowercase())
            .or_else(|| magnet_hash(&blob))
        else {
            continue;
        };
        if title.is_empty() || hash.is_empty() {
            continue;
        }
        let size = size_re
            .captures(&desc)
            .map(|c| parse_size(&c[1]))
            .unwrap_or(0);
        let date = date_re
            .captures(item)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        out.push(torrent_from_parts(
            title,
            hash,
            "tokyotosho",
            "medium",
            0,
            0,
            0,
            size,
            date,
        ));
    }
    out
}

pub fn piratebay_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    const BASE: &str = "https://torrent-search-api-livid.vercel.app/api/piratebay/";
    let title = input.query.titles.first().cloned().unwrap_or_default();
    if title.is_empty() {
        return Ok(vec![]);
    }
    let mut q = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    q = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if input.query.episode > 0 {
        q.push_str(&format!(" {:02}", input.query.episode));
    }
    let url = format!("{BASE}{}", encode_query(&q));
    let rows: Vec<PbRow> = serde_json::from_str(&http_get(&url).unwrap_or_default()).unwrap_or_default();
    Ok(rows.into_iter().filter_map(PbRow::into_result).collect())
}

pub fn piratebay_test() -> bool {
    http_ok("https://torrent-search-api-livid.vercel.app/api/piratebay/one%20piece")
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct PbRow {
    #[serde(rename = "Name", alias = "name")]
    name: Option<String>,
    #[serde(rename = "Magnet", alias = "magnet")]
    magnet: Option<String>,
    #[serde(rename = "Seeders", alias = "seeders")]
    seeders: Option<ValueOrNum>,
    #[serde(rename = "Leechers", alias = "leechers")]
    leechers: Option<ValueOrNum>,
    #[serde(rename = "Downloads", alias = "downloads")]
    downloads: Option<ValueOrNum>,
    #[serde(rename = "Size", alias = "size")]
    size: Option<String>,
    #[serde(rename = "DateUploaded", alias = "date")]
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ValueOrNum {
    N(i64),
    S(String),
}

impl ValueOrNum {
    fn as_i32(&self) -> i32 {
        match self {
            Self::N(n) => *n as i32,
            Self::S(s) => s.parse().unwrap_or(0),
        }
    }
}

impl PbRow {
    fn into_result(self) -> Option<Hit> {
        let title = self.name.unwrap_or_default();
        let magnet = self.magnet.unwrap_or_default();
        let hash = magnet_hash(&magnet).unwrap_or_default();
        if title.is_empty() || hash.is_empty() {
            return None;
        }
        let mut row = torrent_from_parts(
            title,
            hash,
            "piratebay",
            "medium",
            self.seeders.map(|v| v.as_i32()).unwrap_or(0),
            self.leechers.map(|v| v.as_i32()).unwrap_or(0),
            self.downloads.map(|v| v.as_i32()).unwrap_or(0),
            self.size.as_deref().map(parse_size).unwrap_or(0),
            self.date.unwrap_or_default(),
        );
        row.kind = Some("alt".into());
        if magnet.starts_with("magnet:") {
            row.link = magnet;
        }
        Some(row)
    }
}

pub fn nekobt_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    const QUALITIES: [&str; 4] = ["1080", "720", "540", "480"];
    let query = &input.query;
    let mut params = vec!["limit=1".to_string()];
    if let Some(tvdb) = query.tvdb_id.filter(|id| *id > 0) {
        params.push(format!("tvdbid={tvdb}"));
    }
    if let Some(tmdb) = query.tmdb_id.as_deref().filter(|s| !s.is_empty()) {
        params.push(format!("tmdbid={}", encode_query(tmdb)));
    }
    if params.len() == 1 {
        return Ok(vec![]);
    }
    let data = nekobt_fetch(&format!("torrents/search?{}", params.join("&")))?;
    let media = data.get("media").cloned().unwrap_or(Value::Null);
    let media_id = media.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    if media_id == 0 {
        return Ok(vec![]);
    }
    let episodes = media
        .get("episodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let ep = episodes
        .iter()
        .find(|e| {
            query
                .tvdb_eid
                .filter(|id| *id > 0)
                .zip(e.get("tvdbId").and_then(|v| v.as_i64()).map(|n| n as i32))
                .is_some_and(|(want, got)| want == got)
        })
        .or_else(|| {
            episodes.iter().find(|e| {
                e.get("episode")
                    .and_then(|v| v.as_i64())
                    .is_some_and(|n| n as i32 == query.episode)
            })
        });
    let Some(ep) = ep else {
        return Ok(vec![]);
    };
    let ep_id = ep.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    if ep_id == 0 {
        return Ok(vec![]);
    }
    let high = query.tvdb_eid.filter(|id| *id > 0).is_some()
        && ep.get("tvdbId").and_then(|v| v.as_i64()).map(|n| n as i32) == query.tvdb_eid;
    let data = nekobt_fetch(&format!(
        "torrents/search?media_id={media_id}&fansub_lang=en,enm&sub_lang=en,enm&episode_ids={ep_id}"
    ))?;
    let results = data
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut excl: Vec<String> = query
        .exclusions
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if !query.resolution.is_empty() && query.resolution != "0" {
        for q in QUALITIES {
            if q != query.resolution {
                excl.push(format!("{q}p"));
            }
        }
    }
    Ok(results
        .into_iter()
        .filter_map(|entry| map_nekobt(entry, high, &excl))
        .collect())
}

pub fn nekobt_test() -> bool {
    http_ok("https://nekobt.to/api/v1/announcements")
}

fn nekobt_fetch(path_and_query: &str) -> Result<Value, String> {
    let json = json_get(&format!("https://nekobt.to/api/v1/{path_and_query}"))?;
    if json.get("error").and_then(|v| v.as_bool()).unwrap_or(false) {
        let msg = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("NekoBT error");
        return Err(format!("NekoBT: {msg}"));
    }
    Ok(json.get("data").cloned().unwrap_or(Value::Null))
}

fn map_nekobt(entry: Value, high: bool, excl: &[String]) -> Option<Hit> {
    let title = entry.get("title")?.as_str()?.to_string();
    let lower = title.to_ascii_lowercase();
    if excl.iter().any(|e| !e.is_empty() && lower.contains(e)) {
        return None;
    }
    let id = entry.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    let hash = entry
        .get("infohash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if hash.len() < 32 {
        return None;
    }
    let mut row = torrent_from_parts(
        title,
        hash,
        "nekobt",
        if high { "high" } else { "medium" },
        json_num(&entry, "seeders"),
        json_num(&entry, "leechers"),
        json_num(&entry, "completed"),
        entry
            .get("filesize")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                entry
                    .get("filesize")
                    .and_then(|v| v.as_i64())
                    .map(|n| n as u64)
            })
            .unwrap_or(0),
        entry
            .get("uploaded_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    );
    if id > 0 {
        row.link = format!("https://nekobt.to/api/v1/torrents/{id}/download?public=true");
    }
    let level = entry.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
    let batch = entry.get("batch").and_then(|v| v.as_bool()).unwrap_or(false);
    row.kind = if level >= 3 {
        Some("alt".into())
    } else if batch {
        Some("batch".into())
    } else {
        None
    };
    Some(row)
}

fn json_num(entry: &Value, key: &str) -> i32 {
    entry
        .get(key)
        .and_then(|v| v.as_i64())
        .or_else(|| {
            entry
                .get(key)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0) as i32
}

pub fn anisearch_search(input: &SearchInput) -> Result<Vec<Hit>, String> {
    const BASE: &str = "https://api.anisearch.org/torrents?";
    const QUALITIES: [&str; 4] = ["1080", "720", "540", "480"];
    let query = &input.query;
    let after = query
        .after_iso
        .clone()
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".into());
    let mut name_filters = Vec::new();
    for e in &query.exclusions {
        if !e.is_empty() {
            name_filters.push(format!("not.ilike.*{e}*"));
        }
    }
    if !query.resolution.is_empty() && query.resolution != "0" {
        for q in QUALITIES {
            if q != query.resolution {
                name_filters.push(format!("not.ilike.*{q}*"));
            }
        }
    }
    let fetch = |pairs: &[(&str, String)], batch: bool| -> Vec<Hit> {
        let mut qs = Vec::new();
        for (k, v) in pairs {
            qs.push(format!("{}={}", encode_query(k), encode_query(v)));
        }
        for name in &name_filters {
            qs.push(format!("name={}", encode_query(name)));
        }
        let url = format!("{BASE}{}", qs.join("&"));
        let rows: Vec<AsRow> =
            serde_json::from_str(&http_get(&url).unwrap_or_default()).unwrap_or_default();
        rows.into_iter()
            .filter_map(|row| row.into_result(batch))
            .collect()
    };
    let mut out = Vec::new();
    if let Some(eid) = query.anidb_eid.filter(|id| *id > 0) {
        out.extend(fetch(
            &[
                ("eid", eid.to_string()),
                ("includeFiles", "false".into()),
                ("after", after.clone()),
            ],
            false,
        ));
    }
    if !query.is_single && !query.is_movie {
        if let Some(aid) = query.anidb_aid.filter(|id| *id > 0) {
            let file_count = 24.min(2.max(query.episode.max(1)));
            out.extend(fetch(
                &[
                    ("aid", aid.to_string()),
                    ("fileCount", format!("gte.{file_count}")),
                    ("includeFiles", "false".into()),
                    ("after", after.clone()),
                ],
                true,
            ));
        }
    }
    if query.is_movie {
        if let Some(aid) = query.anidb_aid.filter(|id| *id > 0) {
            out.extend(fetch(
                &[
                    ("aid", aid.to_string()),
                    ("includeFiles", "false".into()),
                    ("after", after),
                ],
                false,
            ));
        }
    }
    Ok(out)
}

pub fn anisearch_test() -> bool {
    http_ok("https://api.anisearch.org/torrents?")
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AsRow {
    torrent_name: Option<String>,
    release_name: Option<String>,
    torrent_file_url: Option<String>,
    infohash: Option<String>,
    length: Option<u64>,
    created_at: Option<String>,
}

impl AsRow {
    fn into_result(self, batch: bool) -> Option<Hit> {
        let title = self.torrent_name.or(self.release_name).unwrap_or_default();
        let hash = self.infohash.unwrap_or_default().to_ascii_lowercase();
        if title.is_empty() || hash.len() < 32 {
            return None;
        }
        let mut row = torrent_from_parts(
            title,
            hash,
            "anisearch",
            "medium",
            0,
            0,
            0,
            self.length.unwrap_or(0),
            self.created_at.unwrap_or_default(),
        );
        if let Some(url) = self.torrent_file_url.filter(|u| !u.is_empty()) {
            row.link = url;
        }
        if batch {
            row.kind = Some("batch".into());
        }
        Some(row)
    }
}
