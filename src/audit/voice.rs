//! Voice search / Speakable signals.
//!
//! `featured_snippet_candidate` is the first self-contained 30-50 word
//! passage on the page — what Google reads aloud for a featured snippet
//! or voice assistant answer. `speakable_eligible` flags whether the page
//! already advertises Speakable schema, the explicit voice-assistant hint.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

#[derive(Serialize)]
pub struct Voice {
    pub featured_snippet_candidate: Option<String>,
    pub speakable_eligible: bool,
    pub avg_sentence_words: u32,
}

// Rust's `regex` crate has no look-behind, so we split on punctuation-then-
// whitespace. Each chunk is a sentence body (trailing `.`/`!`/`?` already
// consumed by the split). Good enough for word-count windows.
static SENTENCE_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.!?]+\s+").unwrap());

pub fn extract(body_text: &str, schema_types: &[String], raw_html: &str) -> Voice {
    let sentences: Vec<&str> = SENTENCE_SPLIT
        .split(body_text)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let total_words: usize = sentences.iter().map(|s| s.split_whitespace().count()).sum();
    let avg = if sentences.is_empty() {
        0
    } else {
        (total_words as f64 / sentences.len() as f64).round() as u32
    };

    let candidate = find_snippet_candidate(&sentences);

    let speakable_eligible = schema_types.iter().any(|t| t == "SpeakableSpecification")
        || raw_html.contains("class=\"speakable\"")
        || raw_html.contains("class='speakable'");

    Voice {
        featured_snippet_candidate: candidate,
        speakable_eligible,
        avg_sentence_words: avg,
    }
}

fn find_snippet_candidate(sentences: &[&str]) -> Option<String> {
    // Walk forward accumulating sentences. The first window whose word
    // count falls in [30, 50] wins.
    for i in 0..sentences.len() {
        let mut acc: Vec<&str> = Vec::new();
        let mut words = 0;
        for s in &sentences[i..] {
            let w = s.split_whitespace().count();
            words += w;
            acc.push(s);
            if words >= 30 && words <= 50 {
                return Some(acc.join(" "));
            }
            if words > 50 {
                break;
            }
        }
    }
    None
}
