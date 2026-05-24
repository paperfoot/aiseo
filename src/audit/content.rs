//! Content structure extraction: headings, body text, word count, and the
//! presence flags the suggestions module reasons over.

use once_cell::sync::Lazy;
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
    /// Plain-text body, normalised whitespace. Kept here so downstream
    /// modules (position bias, suggestions) don't re-extract it.
    #[serde(skip_serializing)]
    pub body_text: String,
}

static CREDENTIAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(MD|PhD|Ph\.D\.|M\.D\.|MBA|MSc|MPH|DDS|DMD|JD|RN|DO|DPM|OD|PharmD|DVM|EdD|PsyD)\b")
        .unwrap()
});
static TLDR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)TL;?DR:?\s*").unwrap());
static FAQ_HEADING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)FAQ|Frequently\s+Asked\s+Questions").unwrap());
static AUTHOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)author|by\s+[A-Z]|written\s+by").unwrap());

pub fn extract(doc: &Html) -> ContentStructure {
    let body_text = extract_body_text(doc);

    let h1 = headings(doc, "h1");
    let h2 = headings(doc, "h2");
    let h3 = headings(doc, "h3");

    let heading_blob = format!("{} {} {}", h1.join(" "), h2.join(" "), h3.join(" "));

    ContentStructure {
        word_count: count_words(&body_text),
        has_tldr: TLDR_RE.is_match(&body_text),
        has_faq: FAQ_HEADING_RE.is_match(&heading_blob),
        has_author: AUTHOR_RE.is_match(&body_text),
        has_credentials: CREDENTIAL_RE.is_match(&body_text),
        h1,
        h2,
        h3,
        body_text,
    }
}

fn headings(doc: &Html, tag: &str) -> Vec<String> {
    let sel = Selector::parse(tag).unwrap();
    doc.select(&sel)
        .map(|el| el.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

fn extract_body_text(doc: &Html) -> String {
    let sel = Selector::parse("body").unwrap();
    let body = doc.select(&sel).next();
    let raw: String = match body {
        Some(b) => b.text().collect::<Vec<_>>().join(" "),
        None => doc.root_element().text().collect::<Vec<_>>().join(" "),
    };
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn count_words(s: &str) -> usize {
    s.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count()
}
