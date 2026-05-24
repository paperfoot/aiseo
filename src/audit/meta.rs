//! Extract `<title>`, meta tags, Open Graph, Twitter Card, and JSON-LD
//! schema types from a parsed HTML document.

use scraper::{Html, Selector};
use serde::Serialize;

#[derive(Serialize, Default)]
pub struct Meta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub author: Option<String>,
    pub canonical: Option<String>,
    /// `<meta name="robots">` content. Absence is fine (defaults to
    /// index,follow); presence with `noindex` or `nofollow` is the finding.
    pub robots: Option<String>,
    /// `<meta name="viewport">` content. Absence on a mobile-targeted page
    /// is an SEO/UX finding.
    pub viewport: Option<String>,
    /// True when any `<link rel>` value contains `icon` (`icon`,
    /// `shortcut icon`, `apple-touch-icon`). Branding signal.
    pub favicon: bool,
}

#[derive(Serialize, Default)]
pub struct OpenGraph {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "type")]
    pub og_type: Option<String>,
}

#[derive(Serialize, Default)]
pub struct TwitterCard {
    pub card: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
}

pub fn extract(doc: &Html) -> Meta {
    let mut m = Meta::default();

    let title_sel = Selector::parse("title").unwrap();
    m.title = doc
        .select(&title_sel)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string());

    let meta_sel = Selector::parse("meta").unwrap();
    for el in doc.select(&meta_sel) {
        let name = el.value().attr("name").map(str::to_ascii_lowercase);
        let content = el.value().attr("content").map(str::to_string);
        match (name.as_deref(), content) {
            (Some("description"), Some(v)) => m.description = Some(v),
            (Some("keywords"), Some(v)) => m.keywords = Some(v),
            (Some("author"), Some(v)) => m.author = Some(v),
            (Some("robots"), Some(v)) => m.robots = Some(v),
            (Some("viewport"), Some(v)) => m.viewport = Some(v),
            _ => {}
        }
    }

    let link_sel = Selector::parse("link[rel=\"canonical\"]").unwrap();
    m.canonical = doc
        .select(&link_sel)
        .next()
        .and_then(|el| el.value().attr("href").map(str::to_string));

    // Favicon: any <link rel> that mentions "icon" counts (covers
    // "icon", "shortcut icon", "apple-touch-icon", "mask-icon").
    let any_link_sel = Selector::parse("link[rel]").unwrap();
    m.favicon = doc
        .select(&any_link_sel)
        .any(|el| el.value().attr("rel").is_some_and(|r| r.to_ascii_lowercase().contains("icon")));

    m
}

pub fn extract_open_graph(doc: &Html) -> OpenGraph {
    let mut og = OpenGraph::default();
    let sel = Selector::parse("meta[property]").unwrap();
    for el in doc.select(&sel) {
        let prop = el.value().attr("property").unwrap_or("").to_ascii_lowercase();
        let content = el.value().attr("content").map(str::to_string);
        match (prop.as_str(), content) {
            ("og:title", Some(v)) => og.title = Some(v),
            ("og:description", Some(v)) => og.description = Some(v),
            ("og:image", Some(v)) => og.image = Some(v),
            ("og:url", Some(v)) => og.url = Some(v),
            ("og:type", Some(v)) => og.og_type = Some(v),
            _ => {}
        }
    }
    og
}

pub fn extract_twitter_card(doc: &Html) -> TwitterCard {
    let mut tw = TwitterCard::default();
    let sel = Selector::parse("meta[name]").unwrap();
    for el in doc.select(&sel) {
        let name = el.value().attr("name").unwrap_or("").to_ascii_lowercase();
        let content = el.value().attr("content").map(str::to_string);
        match (name.as_str(), content) {
            ("twitter:card", Some(v)) => tw.card = Some(v),
            ("twitter:title", Some(v)) => tw.title = Some(v),
            ("twitter:description", Some(v)) => tw.description = Some(v),
            ("twitter:image", Some(v)) => tw.image = Some(v),
            _ => {}
        }
    }
    tw
}

/// Pulls `@type` values out of every `<script type="application/ld+json">`
/// block. Multiple `@type`s per block (array form) are flattened. Blocks that
/// don't parse are silently skipped — auditing should not crash on bad
/// JSON-LD; the auditor flags it via the suggestions list instead.
pub fn extract_schema_types(doc: &Html) -> Vec<String> {
    let sel = Selector::parse("script[type=\"application/ld+json\"]").unwrap();
    let mut types = Vec::new();
    for el in doc.select(&sel) {
        let text: String = el.text().collect();
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        collect_types(&val, &mut types);
    }
    types.sort();
    types.dedup();
    types
}

fn collect_types(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(t) = map.get("@type") {
                push_type(t, out);
            }
            for (_, v) in map {
                collect_types(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_types(v, out);
            }
        }
        _ => {}
    }
}

fn push_type(t: &serde_json::Value, out: &mut Vec<String>) {
    match t {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(arr) => {
            for v in arr {
                if let Some(s) = v.as_str() {
                    out.push(s.to_string());
                }
            }
        }
        _ => {}
    }
}
