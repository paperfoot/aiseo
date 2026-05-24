//! Keyword frequency and question extraction.
//!
//! Deliberately heuristic: token-counting + stop-word filter + question-form
//! sentence detection. An agent uses `primary` to decide whether the page's
//! actual top keyword matches the keyword the user is trying to rank for.

use std::sync::LazyLock;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Serialize)]
pub struct Keywords {
    /// Top single-word terms by frequency, after stop-word filtering.
    pub primary: Vec<KeywordHit>,
    /// Sentences that look like questions (end in `?`). Drives FAQ schema
    /// generation decisions.
    pub questions: Vec<String>,
    /// Top-N density: term → percentage of total non-stop tokens.
    pub density: BTreeMap<String, f64>,
}

#[derive(Serialize)]
pub struct KeywordHit {
    pub term: String,
    pub count: usize,
}

static STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "am", "an", "and",
    "any", "are", "as", "at", "be", "because", "been", "before", "being", "below",
    "between", "both", "but", "by", "can", "did", "do", "does", "doing", "down",
    "during", "each", "few", "for", "from", "further", "had", "has", "have",
    "having", "he", "her", "here", "hers", "herself", "him", "himself", "his",
    "how", "i", "if", "in", "into", "is", "it", "its", "itself", "just", "me",
    "might", "more", "most", "my", "myself", "no", "nor", "not", "now", "of",
    "off", "on", "once", "only", "or", "other", "our", "ours", "ourselves", "out",
    "over", "own", "same", "she", "should", "so", "some", "such", "than", "that",
    "the", "their", "theirs", "them", "themselves", "then", "there", "these",
    "they", "this", "those", "through", "to", "too", "under", "until", "up",
    "very", "was", "we", "were", "what", "when", "where", "which", "while", "who",
    "whom", "why", "will", "with", "would", "you", "your", "yours", "yourself",
    "yourselves",
];

static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[a-zA-Z][a-zA-Z\-']{2,}\b").unwrap());
// Real questions only: a sentence ending in `?` whose first word is a
// genuine interrogative ("What/Why/How/When/Where/Who/Which/Is/Are/Do/
// Does/Can/Should/Will/Could/Would/May/Might"). The previous detector
// appended `?` to any sentence starting with "what/how/is/are/can"
// which fabricated questions out of statements; a naïve `[A-Z]…\?` regex
// over-grabs across run-together nav text without punctuation.
static QUESTION_SENTENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)(?:^|[.!?]\s+|\n\s*)((?:What|Why|How|When|Where|Who|Which|Is|Are|Do|Does|Can|Should|Will|Could|Would|May|Might)\b[^.!?\n]{3,180}\?)",
    )
    .unwrap()
});

pub fn extract(body_text: &str) -> Keywords {
    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total: usize = 0;

    for m in WORD.find_iter(body_text) {
        let w = m.as_str().to_ascii_lowercase();
        if stop.contains(w.as_str()) {
            continue;
        }
        total += 1;
        *counts.entry(w).or_insert(0) += 1;
    }

    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let primary: Vec<KeywordHit> = sorted
        .iter()
        .take(5)
        .map(|(t, c)| KeywordHit {
            term: t.clone(),
            count: *c,
        })
        .collect();

    let mut density: BTreeMap<String, f64> = BTreeMap::new();
    if total > 0 {
        for (t, c) in sorted.iter().take(5) {
            density.insert(t.clone(), ((*c as f64 / total as f64) * 1000.0).round() / 10.0);
        }
    }

    // Only sentences that actually end with `?` and start with an
    // interrogative count as questions. Capture group 1 holds the
    // question itself (without the leading sentence-boundary prefix).
    let mut seen_q: HashSet<String> = HashSet::new();
    let questions: Vec<String> = QUESTION_SENTENCE
        .captures_iter(body_text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .map(|q| q.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|q| seen_q.insert(q.to_ascii_lowercase()))
        .take(10)
        .collect();

    Keywords {
        primary,
        questions,
        density,
    }
}
