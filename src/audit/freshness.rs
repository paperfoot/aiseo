//! Freshness signals: dateModified / datePublished from JSON-LD,
//! visible-text "Updated / Modified / Reviewed" strings, `<time
//! datetime>` attributes, year mentions, and the agreement between
//! schema dates and visible dates (Codex flagged the gap: a page can
//! claim dateModified=2026-05 in JSON-LD while the visible header still
//! reads "Updated January 2024" — readers and AI retrievers both notice).

use chrono::{Datelike, NaiveDate, Utc};
use std::sync::LazyLock;
use regex::Regex;
use serde::Serialize;

static DATE_MODIFIED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""dateModified"\s*:\s*"([^"]+)""#).unwrap());
static DATE_PUBLISHED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""datePublished"\s*:\s*"([^"]+)""#).unwrap());
// Wide year range so we don't go silent at decade boundaries — same lesson
// learned in the Python skill's 2030 bug.
static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19\d{2}|20\d{2}|21\d{2})\b").unwrap());

// `<time datetime="2026-05-12">` etc. We just grab the attribute value;
// validation lives in parse_date.
static TIME_DATETIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<time\b[^>]*\bdatetime\s*=\s*["']([^"']+)["']"#).unwrap()
});

// Visible "Updated <date>" / "Last updated <date>" / "Modified <date>" /
// "Reviewed <date>" strings. Matches month-day-year, year-only, or
// ISO-like fragments — anything that looks like a date in plain prose.
static VISIBLE_DATE_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:last\s+updated|updated\s+on|updated|last\s+modified|modified\s+on|modified|reviewed\s+on|reviewed|fact[\s-]?checked\s+on|fact[\s-]?checked|posted|published)\s*[:.\s]+\s*((?:\d{1,2}\s+)?(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\.?\s+\d{1,2}?,?\s*\d{4}|\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{2,4}|\d{4})",
    )
    .unwrap()
});

#[derive(Serialize)]
pub struct Freshness {
    pub date_modified: Option<String>,
    pub date_published: Option<String>,
    /// Days since the most recent dateModified or datePublished found in
    /// JSON-LD. None when no date is present.
    pub days_since_modified: Option<i64>,
    pub year_mentions: Vec<u16>,
    pub current_year: i32,
    /// First `<time datetime>` attribute value, if present. Visible-time
    /// elements are the HTML-spec-blessed way to mark up dates and AI
    /// retrievers parse them.
    pub time_datetime: Option<String>,
    /// First visible "Updated <date>" / "Modified <date>" / "Reviewed
    /// <date>" string found in the body text.
    pub visible_updated_label: Option<String>,
    /// True when JSON-LD claims a dateModified that's substantially newer
    /// than the latest visible signal. Hard signal that the schema is
    /// being updated without the visible text being refreshed.
    ///
    /// Codex 2026-05-24: Google's publication-dates guidance is that
    /// visible + structured dates should match — no documented grace
    /// period. We escalate by severity: any parsed mismatch >30 days is
    /// `Mild`, >180 days is `Severe`. The boolean above stays for
    /// backward-compat; the severity field is what suggest.rs reads.
    pub schema_vs_visible_mismatch: bool,
    pub schema_vs_visible_severity: MismatchSeverity,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MismatchSeverity {
    None,
    Mild,
    Severe,
}

pub fn analyze(html: &str, body_text: &str, _schema_types: &[String]) -> Freshness {
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
    let days = pick.and_then(|s| parse_date(s).map(|d| (today - d).num_days()));

    let mut years: Vec<u16> = YEAR_RE
        .find_iter(body_text)
        .filter_map(|m| m.as_str().parse::<u16>().ok())
        .collect();
    years.sort();
    years.dedup();

    let time_datetime = TIME_DATETIME_RE
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let visible_updated_label = VISIBLE_DATE_LABEL_RE
        .captures(body_text)
        .and_then(|c| c.get(0).map(|m| m.as_str().to_string()))
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "));

    // Schema-vs-visible mismatch with severity escalation.
    // - Mild: schema dateModified parses, visible date parses, gap >30 days
    // - Severe: gap >180 days, OR schema-modified present + no visible date
    //   on long content
    let visible_date = time_datetime.as_deref().and_then(parse_date).or_else(|| {
        visible_updated_label
            .as_deref()
            .and_then(parse_visible_label_date)
    });
    let schema_date = date_modified.as_deref().and_then(parse_date);
    let schema_vs_visible_severity = match (schema_date, visible_date) {
        (Some(schema), Some(visible)) => {
            let gap = (schema - visible).num_days();
            if gap > 180 {
                MismatchSeverity::Severe
            } else if gap > 30 {
                MismatchSeverity::Mild
            } else {
                MismatchSeverity::None
            }
        }
        (Some(_), None) if date_modified.is_some() => {
            if body_text.split_whitespace().count() >= 400 {
                MismatchSeverity::Severe
            } else {
                MismatchSeverity::None
            }
        }
        _ => MismatchSeverity::None,
    };
    let schema_vs_visible_mismatch =
        !matches!(schema_vs_visible_severity, MismatchSeverity::None);

    Freshness {
        date_modified,
        date_published,
        days_since_modified: days,
        year_mentions: years,
        current_year: today.year(),
        time_datetime,
        visible_updated_label,
        schema_vs_visible_mismatch,
        schema_vs_visible_severity,
    }
}

/// Parse the date portion of a visible "Updated <date>" label.
///
/// Previously this only extracted the year and treated it as January 1,
/// so `Updated Dec 30 2026` vs schema `2026-12-31` showed a 364-day gap
/// and got flagged as `Severe` when the real gap is 1 day. We now try
/// the common visible-date shapes before falling back to year-only.
fn parse_visible_label_date(label: &str) -> Option<NaiveDate> {
    // The capture from VISIBLE_DATE_LABEL_RE group 1 is the date string
    // without the leading "Updated" / "Modified" / etc. Re-run on the
    // already-matched label to get group 1.
    let date_str = VISIBLE_DATE_LABEL_RE
        .captures(label)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| label.to_string());

    // Try the common shapes the regex admits. Order matters only for
    // ambiguous separators (M/D/Y vs D/M/Y) — we try US first since
    // most English-language pages use it, then European.
    let formats = [
        "%Y-%m-%d",      // 2026-12-31
        "%d %b %Y",      // 30 Dec 2026
        "%d %B %Y",      // 30 December 2026
        "%b %d %Y",      // Dec 30 2026
        "%B %d %Y",      // December 30 2026
        "%b %d, %Y",     // Dec 30, 2026
        "%B %d, %Y",     // December 30, 2026
        "%m/%d/%Y",      // 12/31/2026
        "%d/%m/%Y",      // 31/12/2026
        "%m/%d/%y",      // 12/31/26
        "%d/%m/%y",      // 31/12/26
    ];
    for fmt in &formats {
        if let Ok(d) = NaiveDate::parse_from_str(&date_str, fmt) {
            return Some(d);
        }
    }

    // Fallback: year-only label like "Updated 2026" — keep the old
    // behaviour so we don't lose existing signal.
    YEAR_RE
        .find(&date_str)
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .and_then(|y| NaiveDate::from_ymd_opt(y, 1, 1))
}

/// Parse a JSON-LD date value tolerantly. Handles RFC 3339 with timezone,
/// plain YYYY-MM-DD, and YYYY-MM-DDTHH:MM:SS. Returns None on anything
/// else (including the v0.4 UTF-8-slice panic on non-ASCII inputs).
fn parse_date(s: &str) -> Option<NaiveDate> {
    // RFC 3339 first (handles timezones).
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_utc().date());
    }
    // Then the first 10 ASCII chars as YYYY-MM-DD, but only if we have
    // at least 10 ASCII chars at the front — protects against the UTF-8
    // slice panic from v0.4.
    let head: String = s.chars().take_while(|c| c.is_ascii()).take(10).collect();
    if head.len() == 10 {
        return NaiveDate::parse_from_str(&head, "%Y-%m-%d").ok();
    }
    None
}
