//! Copy-precision — a *positive* score that rewards tight prose.
//!
//! The inverse of slop. Velvet-glove discipline: every word load-bearing,
//! no filler, no throat-clearing, no hedge stacks, concrete nouns over
//! abstract abstractions. Score is 0..10 (high is tight), computed as
//! `10 - sum(penalties).clamp(0, 10)`. Verdict: `tight` ≥ 8, `mid` 5..7,
//! `padded` < 5.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct CopyPrecision {
    pub score: f32,
    pub counts: HashMap<&'static str, usize>,
    pub densities: HashMap<&'static str, f32>,
    pub verdict: &'static str,
}

static FILLER_WORDS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(very|really|extremely|quite|rather|somewhat|fairly|absolutely|basically|literally|actually|essentially|simply|just|truly|obviously|clearly|definitely|certainly|particularly|specifically)\b",
    )
    .unwrap()
});

static LY_ADVERB: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b[a-z]{4,}ly\b").unwrap());
static LY_EXCLUDE: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
    [
        "only", "early", "daily", "weekly", "monthly", "yearly", "family",
        "really", "fully", "july", "italy",
    ]
    .into_iter()
    .collect()
});

static HEDGED_MODALS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(might|could|may|should|would|perhaps|possibly|potentially|presumably|arguably)\b",
    )
    .unwrap()
});

static EMPTY_EMPHASIS_ADJ: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(crucial|essential|important|key|vital|critical|significant|substantial|noteworthy|remarkable|notable|paramount|pivotal)\b",
    )
    .unwrap()
});

static THROAT_CLEARING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?im)^(?:in order to|it is also the case that|one of the things that|what this means is|the fact is that|it should be (?:noted|said))\b",
    )
    .unwrap()
});

static FILLER_PHRASES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(at the end of the day|when all is said and done|in many ways|to a large extent|on a personal level|the fact of the matter is|the bottom line is)\b",
    )
    .unwrap()
});

static PASSIVE_VOICE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:is|was|are|were|been|being)\s+\w+(?:ed|en)\b").unwrap()
});

static PROPER_NOUN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[A-Z][a-z]{2,}\b").unwrap());
static NUMERIC: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d+(?:[.,]\d+)?(?:%|x|×)?\b").unwrap());

static SENTENCE_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.!?]+\s+").unwrap());

fn count_ly_excluding(text: &str) -> usize {
    LY_ADVERB
        .find_iter(text)
        .filter(|m| !LY_EXCLUDE.contains(m.as_str().to_ascii_lowercase().as_str()))
        .count()
}

pub fn extract(body_text: &str) -> CopyPrecision {
    let words: Vec<&str> = body_text.split_whitespace().collect();

    // Below this threshold the density math degenerates: zero filler in an
    // empty body trivially yields `score: 10, verdict: tight`, which is a
    // lie. Bail out with an honest verdict instead.
    if words.len() < 20 {
        let mut counts: HashMap<&'static str, usize> = HashMap::new();
        counts.insert("words", words.len());
        return CopyPrecision {
            score: 0.0,
            counts,
            densities: HashMap::new(),
            verdict: "insufficient_content",
        };
    }

    let word_count = words.len();
    let per_1k = |n: usize| -> f32 { (n as f32 * 1000.0 / word_count as f32 * 10.0).round() / 10.0 };

    let filler = FILLER_WORDS.find_iter(body_text).count();
    let ly = count_ly_excluding(body_text);
    let hedges = HEDGED_MODALS.find_iter(body_text).count();
    let empty_adj = EMPTY_EMPHASIS_ADJ.find_iter(body_text).count();
    let throat = THROAT_CLEARING.find_iter(body_text).count();
    let phrases = FILLER_PHRASES.find_iter(body_text).count();
    let passive = PASSIVE_VOICE.find_iter(body_text).count();

    // Sentence-length variance: σ/μ
    let sentences: Vec<usize> = SENTENCE_SPLIT
        .split(body_text)
        .map(|s| s.split_whitespace().count())
        .filter(|n| *n > 0)
        .collect();
    let var_ratio = if sentences.len() >= 4 {
        let mean: f64 =
            sentences.iter().map(|n| *n as f64).sum::<f64>() / sentences.len() as f64;
        let var: f64 = sentences
            .iter()
            .map(|n| (*n as f64 - mean).powi(2))
            .sum::<f64>()
            / sentences.len() as f64;
        let sd = var.sqrt();
        if mean > 0.0 { sd / mean } else { 0.0 }
    } else {
        0.45 // not enough data → assume mid
    };

    // Average word length (in characters)
    let avg_word_len: f64 = if word_count > 0 {
        words.iter().map(|w| w.chars().count()).sum::<usize>() as f64
            / word_count as f64
    } else {
        0.0
    };

    // Concrete-noun proxy per 100 words
    let proper_count = PROPER_NOUN.find_iter(body_text).count();
    let numeric_count = NUMERIC.find_iter(body_text).count();
    let concrete_per_100 = (proper_count + numeric_count) as f32 * 100.0 / word_count as f32;

    // ── Penalties (capped at 10 total) ─────────────────────────────────────
    let mut penalty = 0.0_f32;

    let filler_d = per_1k(filler);
    if filler_d > 20.0 {
        penalty += 3.0;
    } else if filler_d > 10.0 {
        penalty += 2.0;
    }

    let ly_d = per_1k(ly);
    if ly_d > 25.0 {
        penalty += 2.0;
    } else if ly_d > 15.0 {
        penalty += 1.0;
    }

    let hedge_d = per_1k(hedges);
    if hedge_d > 35.0 {
        penalty += 2.0;
    } else if hedge_d > 20.0 {
        penalty += 1.0;
    }

    let empty_d = per_1k(empty_adj);
    if empty_d > 15.0 {
        penalty += 2.0;
    } else if empty_d > 8.0 {
        penalty += 1.0;
    }

    penalty += (throat as f32).min(3.0);
    penalty += (phrases as f32).min(3.0);

    if var_ratio < 0.30 && sentences.len() >= 6 {
        penalty += 2.0;
    }

    if avg_word_len > 5.8 {
        penalty += 2.0;
    } else if avg_word_len > 5.4 {
        penalty += 1.0;
    }

    if concrete_per_100 < 2.0 && word_count >= 200 {
        penalty += 2.0;
    } else if concrete_per_100 > 5.0 {
        // Reward: shave half a point of accumulated penalty.
        penalty = (penalty - 0.5).max(0.0);
    }

    let passive_d = per_1k(passive);
    if passive_d > 25.0 {
        penalty += 2.0;
    } else if passive_d > 15.0 {
        penalty += 1.0;
    }

    let score = (10.0 - penalty.min(10.0)).max(0.0);
    let verdict = if score >= 8.0 {
        "tight"
    } else if score >= 5.0 {
        "mid"
    } else {
        "padded"
    };

    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    counts.insert("filler_words", filler);
    counts.insert("ly_adverbs", ly);
    counts.insert("hedged_modals", hedges);
    counts.insert("empty_emphasis_adjectives", empty_adj);
    counts.insert("throat_clearing_openers", throat);
    counts.insert("filler_phrases", phrases);
    counts.insert("passive_voice", passive);
    counts.insert("proper_nouns", proper_count);
    counts.insert("numerics", numeric_count);
    counts.insert("sentences", sentences.len());

    let mut densities: HashMap<&'static str, f32> = HashMap::new();
    densities.insert("filler_per_1k", filler_d);
    densities.insert("ly_per_1k", ly_d);
    densities.insert("hedge_per_1k", hedge_d);
    densities.insert("empty_emphasis_per_1k", empty_d);
    densities.insert("passive_per_1k", passive_d);
    densities.insert(
        "sentence_length_var_ratio",
        (var_ratio * 1000.0).round() as f32 / 1000.0,
    );
    densities.insert(
        "avg_word_length_chars",
        (avg_word_len * 100.0).round() as f32 / 100.0,
    );
    densities.insert(
        "concrete_per_100_words",
        (concrete_per_100 * 100.0).round() as f32 / 100.0,
    );

    CopyPrecision {
        score: (score * 10.0).round() / 10.0,
        counts,
        densities,
        verdict,
    }
}

pub fn suggestion(cp: &CopyPrecision) -> Option<String> {
    match cp.verdict {
        "tight" | "insufficient_content" => None,
        "mid" => Some(format!(
            "Copy precision {:.1}/10 (mid). Cut filler words and empty-emphasis adjectives; favour concrete nouns and numbers.",
            cp.score
        )),
        "padded" => Some(format!(
            "Copy precision {:.1}/10 (padded). Heavy filler / hedge / empty-emphasis density. Rewrite for load-bearing words only.",
            cp.score
        )),
        _ => None,
    }
}
