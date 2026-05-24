//! Evidence-density signals.
//!
//! Counts statistics, quote-like blocks, and flags claims that look like
//! assertions but aren't backed by a citation marker. The validated 2026
//! GEO tactics — statistics addition (+41%), named-authority quotes
//! (+28%) — depend on these being present and visible.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

#[derive(Serialize)]
pub struct Evidence {
    pub stat_count: usize,
    pub quote_count: usize,
    pub unsupported_claims: Vec<UnsupportedClaim>,
}

#[derive(Serialize)]
pub struct UnsupportedClaim {
    pub snippet: String,
    /// Word-offset percentage where the claim sits in the body.
    pub position_pct: f64,
}

static STAT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(\d+(?:,\d{3})*(?:\.\d+)?)\s*(%|percent|x|×|fold|years?|months?|days?|patients?|people|users?|cases?|million|billion)\b",
    )
    .unwrap()
});

static QUOTE: Lazy<Regex> = Lazy::new(|| {
    // Straight or curly double-quoted blocks of at least 5 words.
    Regex::new(r#"["“]([^"”]{20,})["”]"#).unwrap()
});

// "Studies show…" "Research indicates…" "Experts agree…" — claim-shaped
// sentences. We flag the ones that don't have a citation marker ([1], (2024),
// or an http link) in the next ~140 chars.
static CLAIM_INTRO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(studies\s+show|research\s+(?:shows?|indicates?|suggests?|finds?)|evidence\s+suggests?|experts\s+agree|data\s+(?:shows?|indicates?)|it\s+is\s+well\s+known|widely\s+accepted)\b",
    )
    .unwrap()
});

static CITATION_MARKER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(\[\d+\]|\(\d{4}\)|https?://)").unwrap());

pub fn extract(body_text: &str) -> Evidence {
    let stat_count = STAT.find_iter(body_text).count();
    let quote_count = QUOTE.find_iter(body_text).count();

    let total_words = body_text.split_whitespace().count().max(1) as f64;
    let mut unsupported: Vec<UnsupportedClaim> = Vec::new();

    for m in CLAIM_INTRO.find_iter(body_text) {
        let claim_start = m.start();
        let window_end = (claim_start + 200).min(body_text.len());
        let window = &body_text[claim_start..window_end];

        if CITATION_MARKER.is_match(window) {
            continue;
        }

        let preceding = body_text[..claim_start].split_whitespace().count();
        let position_pct = ((preceding as f64 / total_words) * 1000.0).round() / 10.0;

        let snippet_end = (claim_start + 120).min(body_text.len());
        // Avoid splitting inside a UTF-8 character on the trailing edge.
        let snippet_end = (claim_start..=snippet_end)
            .rev()
            .find(|&i| body_text.is_char_boundary(i))
            .unwrap_or(claim_start);
        let snippet: String = body_text[claim_start..snippet_end]
            .chars()
            .take(120)
            .collect::<String>()
            .trim()
            .to_string();

        unsupported.push(UnsupportedClaim {
            snippet,
            position_pct,
        });
        if unsupported.len() >= 10 {
            break;
        }
    }

    Evidence {
        stat_count,
        quote_count,
        unsupported_claims: unsupported,
    }
}
