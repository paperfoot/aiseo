//! Freshness signals: presence of dateModified / datePublished, year
//! mentions in the body, and whether the page is plausibly current.

use chrono::{Datelike, NaiveDate, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

static DATE_MODIFIED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""dateModified"\s*:\s*"([^"]+)""#).unwrap());
static DATE_PUBLISHED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""datePublished"\s*:\s*"([^"]+)""#).unwrap());
// Wide year range so we don't go silent at decade boundaries — same lesson
// learned in the Python skill's 2030 bug.
static YEAR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(19\d{2}|20\d{2}|21\d{2})\b").unwrap());

#[derive(Serialize)]
pub struct Freshness {
    pub date_modified: Option<String>,
    pub date_published: Option<String>,
    /// Days since the most recent dateModified or datePublished found in
    /// JSON-LD. None when no date is present.
    pub days_since_modified: Option<i64>,
    pub year_mentions: Vec<u16>,
    pub current_year: i32,
}

pub fn analyze(html: &str, _schema_types: &[String]) -> Freshness {
    let date_modified = DATE_MODIFIED_RE
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());
    let date_published = DATE_PUBLISHED_RE
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let today = Utc::now().date_naive();
    let pick = date_modified.as_ref().or(date_published.as_ref());
    let days = pick.and_then(|s| {
        // Accept ISO 8601 dates with or without time component.
        NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d")
            .ok()
            .map(|d| (today - d).num_days())
    });

    let mut years: Vec<u16> = YEAR_RE
        .find_iter(html)
        .filter_map(|m| m.as_str().parse::<u16>().ok())
        .collect();
    years.sort();
    years.dedup();

    Freshness {
        date_modified,
        date_published,
        days_since_modified: days,
        year_mentions: years,
        current_year: today.year(),
    }
}
