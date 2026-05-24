//! Content structure extraction: headings, body text, word count, and the
//! presence flags the suggestions module reasons over.

use std::sync::LazyLock;
use regex::Regex;
use scraper::{Html, Selector};
use serde::Serialize;

#[derive(Serialize)]
pub struct ContentStructure {
    pub word_count: usize,
    pub h1: Vec<String>,
    pub h2: Vec<String>,
    pub h3: Vec<String>,
    pub has_tldr: bool,
    pub has_faq: bool,
    pub has_author: bool,
    pub has_credentials: bool,
    /// Number of `<img>` tags without a non-empty `alt` attribute.
    pub missing_alt_count: usize,
    /// Total `<img>` tags for ratio context.
    pub image_count: usize,
    /// `<html lang>` value, if present. Used to suppress English-only
    /// heuristics on non-English pages.
    pub html_lang: Option<String>,
    /// Heading sequence in DOM order, level + text. Drives the
    /// hierarchy-violation check (H3 before H2, skipped levels).
    pub headings_in_order: Vec<HeadingOrderEntry>,
    /// `hreflang` values found on `<link rel="alternate">` — multilingual
    /// SEO signal. Empty list means the page only advertises itself.
    pub hreflangs: Vec<String>,
    /// `<noscript>` body content, if the noscript only contains a
    /// boilerplate "please enable JavaScript" message it's a quality
    /// problem rather than a real fallback.
    pub noscript_kind: NoscriptKind,
    /// `<table>` count outside chrome. Perplexity and AI Mode lift
    /// pages with comparison tables; near-zero on doc-style pages is
    /// a signal to add at least one.
    pub table_count: usize,
    /// Empty headings (whitespace-only `<h1>..<h6>`). Confuse crawlers
    /// and assistive tech — should be deleted, not styled invisible.
    pub empty_heading_count: usize,
    /// Duplicate headings (same text appears under more than one level
    /// or twice at the same level outside chrome). Indicates templated
    /// boilerplate or accidental copy-paste.
    pub duplicate_heading_count: usize,
    /// Sentences in the 5..25 word window with at least one terminal
    /// punctuation. Proxy for direct-quotable sentences — what ChatGPT,
    /// Claude, and Perplexity cite verbatim.
    pub quotable_sentence_count: usize,
    /// Plain-text body, normalised whitespace. Kept here so downstream
    /// modules (position bias, suggestions) don't re-extract it.
    #[serde(skip_serializing)]
    pub body_text: String,
}

#[derive(Serialize)]
pub struct HeadingOrderEntry {
    pub level: u8,
    pub text: String,
}

#[derive(Serialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum NoscriptKind {
    Absent,
    BoilerplateOnly,
    Substantive,
}

// Credentials must follow a capitalised name within ~3 tokens — bare "OD",
// "DO", "JD" sit in normal prose ("the DO loop", "OD optical density",
// "JD Edwards") and false-positived the v0.3 detector.
static CREDENTIAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b[A-Z][a-zA-Z\-']+(?:\s+[A-Z][a-zA-Z\-']+){0,3}\s*,?\s*(MD|Ph\.?D\.?|MBA|MSc|MPH|DDS|DMD|JD|RN|DO|DPM|OD|PharmD|DVM|EdD|PsyD)\b",
    )
    .unwrap()
});
static TLDR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)TL;?DR:?\s*").unwrap());
static FAQ_HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)FAQ|Frequently\s+Asked\s+Questions").unwrap());
static AUTHOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)author|by\s+[A-Z]|written\s+by").unwrap());

pub fn extract(doc: &Html) -> ContentStructure {
    let body_text = extract_body_text(doc);

    // Headings are filtered to drop those nested inside page chrome
    // (<nav>, <header>, <footer>, <aside>) — footer column titles
    // (Company / Programs / Connect) used to false-trigger the
    // hierarchy-skip check on every site with a multi-column footer.
    let h1 = headings(doc, "h1");
    let h2 = headings(doc, "h2");
    let h3 = headings(doc, "h3");

    let heading_blob = format!("{} {} {}", h1.join(" "), h2.join(" "), h3.join(" "));

    let (image_count, missing_alt_count) = count_images(doc);
    let html_lang = extract_html_lang(doc);
    let headings_in_order = extract_headings_in_order(doc);
    let hreflangs = extract_hreflangs(doc);
    let noscript_kind = classify_noscript(doc);
    let table_count = count_tables(doc);
    let (empty_heading_count, duplicate_heading_count) =
        analyze_heading_quality(&headings_in_order);
    let quotable_sentence_count = count_quotable_sentences(&body_text);

    ContentStructure {
        word_count: count_words(&body_text),
        has_tldr: TLDR_RE.is_match(&body_text),
        has_faq: FAQ_HEADING_RE.is_match(&heading_blob),
        has_author: AUTHOR_RE.is_match(&body_text),
        has_credentials: CREDENTIAL_RE.is_match(&body_text),
        h1,
        h2,
        h3,
        image_count,
        missing_alt_count,
        html_lang,
        headings_in_order,
        hreflangs,
        noscript_kind,
        table_count,
        empty_heading_count,
        duplicate_heading_count,
        quotable_sentence_count,
        body_text,
    }
}

fn count_tables(doc: &Html) -> usize {
    let sel = Selector::parse("table").unwrap();
    doc.select(&sel).filter(|el| !is_in_chrome(*el)).count()
}

fn analyze_heading_quality(headings: &[HeadingOrderEntry]) -> (usize, usize) {
    // Empty was filtered upstream, so this counts zero-text headings that
    // somehow survived (defensive). The duplicate count is the real value:
    // identical text at the same or different levels.
    let empty = headings.iter().filter(|h| h.text.trim().is_empty()).count();

    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for h in headings {
        let key = h.text.trim().to_ascii_lowercase();
        if !key.is_empty() {
            *seen.entry(key).or_insert(0) += 1;
        }
    }
    let duplicates = seen.values().filter(|n| **n > 1).map(|n| n - 1).sum();
    (empty, duplicates)
}

static QUOTABLE_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?]+\s+").unwrap());

fn count_quotable_sentences(body_text: &str) -> usize {
    QUOTABLE_SPLIT
        .split(body_text)
        .filter(|s| {
            let words = s.split_whitespace().count();
            // 5..25 word window — too short isn't quotable, too long
            // gets paraphrased.
            (5..=25).contains(&words)
        })
        .count()
}

fn extract_headings_in_order(doc: &Html) -> Vec<HeadingOrderEntry> {
    let sel = Selector::parse("h1, h2, h3, h4, h5, h6").unwrap();
    doc.select(&sel)
        .filter(|el| !is_in_chrome(*el))
        .filter_map(|el| {
            let name = el.value().name();
            let level: u8 = name.strip_prefix('h')?.parse().ok()?;
            let text: String = el
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if text.is_empty() {
                return None;
            }
            Some(HeadingOrderEntry { level, text })
        })
        .collect()
}

/// True when the element sits inside `<nav>`, `<footer>`, `<aside>`, or a
/// banner-style `<header>` (one containing a nav). Article-page hero
/// `<header>` elements are NOT chrome — they typically wrap the H1 + dek
/// and represent real content.
fn is_in_chrome(el: scraper::ElementRef<'_>) -> bool {
    let mut node = el.parent();
    while let Some(n) = node {
        if let scraper::Node::Element(elem) = n.value() {
            match elem.name() {
                "nav" | "footer" | "aside" => return true,
                "header" => {
                    if let Some(eref) = scraper::ElementRef::wrap(n)
                        && header_is_banner(eref)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        node = n.parent();
    }
    false
}

fn extract_hreflangs(doc: &Html) -> Vec<String> {
    let sel = Selector::parse("link[rel=\"alternate\"][hreflang]").unwrap();
    let mut out: Vec<String> = doc
        .select(&sel)
        .filter_map(|el| el.value().attr("hreflang").map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn classify_noscript(doc: &Html) -> NoscriptKind {
    let sel = Selector::parse("noscript").unwrap();
    let mut any_present = false;
    let mut any_substantive = false;
    for el in doc.select(&sel) {
        any_present = true;
        let text: String = el.text().collect::<String>().to_ascii_lowercase();
        let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        // ≥40 chars beyond the boilerplate trigger threshold counts.
        let is_boilerplate = trimmed.contains("enable javascript")
            || trimmed.contains("requires javascript")
            || trimmed.contains("turn on javascript")
            || trimmed.contains("javascript is disabled");
        if !is_boilerplate && trimmed.len() >= 40 {
            any_substantive = true;
        }
    }
    if !any_present {
        NoscriptKind::Absent
    } else if any_substantive {
        NoscriptKind::Substantive
    } else {
        NoscriptKind::BoilerplateOnly
    }
}

fn count_images(doc: &Html) -> (usize, usize) {
    let sel = Selector::parse("img").unwrap();
    let mut total = 0;
    let mut missing = 0;
    for el in doc.select(&sel) {
        total += 1;
        let alt = el.value().attr("alt").unwrap_or("").trim();
        if alt.is_empty() {
            missing += 1;
        }
    }
    (total, missing)
}

fn extract_html_lang(doc: &Html) -> Option<String> {
    let sel = Selector::parse("html").unwrap();
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn headings(doc: &Html, tag: &str) -> Vec<String> {
    let sel = Selector::parse(tag).unwrap();
    doc.select(&sel)
        .filter(|el| !is_in_chrome(*el))
        .map(|el| el.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Visible body text only. `scraper::Html::text()` walks every descendant
/// text node including `<script>` and `<style>` bodies — which the v0.6
/// stress test confirmed was poisoning every prose detector (avg word
/// length of 43 chars on Linear, 33k "words" on a few-hundred-word page).
///
/// We walk the body (or `<main>` if present) ourselves, descending only
/// into nodes whose element name is not in the skip set.
///
/// Skips:
///   - non-prose: script, style, noscript, template, svg, iframe, object, embed
///   - page chrome: nav, header, footer, aside (these poison the
///     featured-snippet candidate with "Skip to main content Home …" and
///     inflate the word count with link text)
fn extract_body_text(doc: &Html) -> String {
    let root_ref = pick_content_root(doc);
    let mut buf = String::with_capacity(8 * 1024);
    let mut stack: Vec<_> = root_ref.children().rev().collect();
    while let Some(node) = stack.pop() {
        match node.value() {
            scraper::Node::Text(t) => {
                buf.push_str(t);
                buf.push(' ');
            }
            scraper::Node::Element(el) => {
                if skip_element(el.name()) {
                    continue;
                }
                // <header> is descended into by default; only a banner-style
                // header (one that contains a <nav>) gets skipped so we don't
                // drop hero text on article pages.
                if el.name() == "header"
                    && let Some(eref) = scraper::ElementRef::wrap(node)
                    && header_is_banner(eref)
                {
                    continue;
                }
                for child in node.children().rev() {
                    stack.push(child);
                }
            }
            _ => {}
        }
    }
    buf.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A `<header>` containing a `<nav>` descendant is a site banner — skip it.
/// A `<header>` without nav is usually an article hero (page title + dek)
/// and counts as content.
fn header_is_banner(el: scraper::ElementRef<'_>) -> bool {
    let nav_sel = Selector::parse("nav").unwrap();
    el.select(&nav_sel).next().is_some()
}

/// Prefer `<main>` over `<body>` when the page declares one — modern
/// templates put real content inside `<main>` and chrome outside it, so
/// rooting the walker there gives a cleaner extraction.
fn pick_content_root(doc: &Html) -> scraper::ElementRef<'_> {
    let main_sel = Selector::parse("main").unwrap();
    if let Some(m) = doc.select(&main_sel).next() {
        return m;
    }
    let body_sel = Selector::parse("body").unwrap();
    doc.select(&body_sel).next().unwrap_or(doc.root_element())
}

fn skip_element(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "style"
            | "noscript"
            | "template"
            | "svg"
            | "iframe"
            | "object"
            | "embed"
            // Page chrome — always skipped.
            | "nav"
            | "footer"
            | "aside"
            // <header> is intentionally absent here. Many article templates
            // wrap the hero (with its real H1) in <header>; skipping it
            // unconditionally would drop the page's primary heading.
            // We handle <header> via is_chrome_header below, which only
            // skips banner-style headers (those containing a <nav>).
    )
}

fn count_words(s: &str) -> usize {
    s.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count()
}
