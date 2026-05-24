//! Metatext detector — catches the *agent-speaking-instead-of-content*
//! class of slop that the lexical `ai_slop` module misses.
//!
//! Three concrete categories:
//!   1. Process narration — "I'll start by…", "Here's a comprehensive…"
//!   2. Self-identification — "As an AI…", "I cannot browse…", bracket asides
//!   3. Closing pleasantries — "Hope this helps", "Feel free to ask"
//!   4. Hedge stacks — "It's worth noting that…", "Please bear in mind…"
//!   5. Markdown envelopes — "Below is a comprehensive…", "Above is…"
//!   6. Question restatement — "You asked about…", "Regarding your query…"
//!   7. Sycophancy — "Great question!"
//!   8. Heading-skeleton Jaccard — page outline matches the canonical AI
//!      table-of-contents (Introduction / Background / Key Features / FAQ /
//!      Conclusion). Novel detector — no existing OSS tool ships it.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
pub struct Metatext {
    pub signals: Vec<Signal>,
    pub heading_skeleton: HeadingSkeleton,
    pub weighted_score_per_1000_words: f64,
    pub verdict: &'static str,
}

#[derive(Serialize, Clone)]
pub struct Signal {
    pub kind: &'static str,
    pub confidence: f64,
    pub snippet: String,
    pub position_pct: f64,
}

#[derive(Serialize)]
pub struct HeadingSkeleton {
    pub jaccard: f64,
    pub matched: Vec<String>,
}

/// Canonical AI table-of-contents — when a page's headings substantially
/// overlap this set, the skeleton is the giveaway.
static CANONICAL_HEADINGS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "introduction",
        "background",
        "overview",
        "getting started",
        "key features",
        "benefits",
        "how it works",
        "implementation",
        "best practices",
        "common pitfalls",
        "use cases",
        "examples",
        "conclusion",
        "summary",
        "key takeaways",
        "faq",
        "frequently asked questions",
        "next steps",
        "further reading",
        "resources",
        "references",
        "tips",
        "key insights",
    ]
    .into_iter()
    .collect()
});

struct Pattern {
    kind: &'static str,
    confidence: f64,
    re: Regex,
    /// Position-weight multipliers for hits in head / mid / tail thirds of
    /// the body. None = uniform 1.0 across the doc.
    weights: Option<(f64, f64, f64)>,
}

fn mk(kind: &'static str, conf: f64, p: &str, weights: Option<(f64, f64, f64)>) -> Pattern {
    Pattern {
        kind,
        confidence: conf,
        re: Regex::new(p).unwrap_or_else(|e| panic!("metatext regex `{kind}`: {e}")),
        weights,
    }
}

static PATTERNS: Lazy<Vec<Pattern>> = Lazy::new(|| {
    vec![
        // ── Process narration (openers cluster in the first 15%) ─────────────
        mk(
            "process_narration_opener",
            0.85,
            r"(?i)\b(?:i'?ll|let me|first,?\s*i'?ll|to begin,?\s*i'?ll)\s+(?:start|begin|outline|walk you|cover|explain|break down|put together|compile|draft)\b",
            Some((2.0, 1.0, 0.5)),
        ),
        mk(
            "process_narration_havedone",
            0.92,
            r"(?i)\bi'?ve\s+(?:put together|compiled|drafted|prepared|outlined|broken down|assembled|gathered)\s+(?:a|an|the|some|this)\b",
            None,
        ),
        mk(
            "comprehensive_opener",
            0.95,
            r"(?i)\bhere'?s\s+(?:a|an|the)\s+(?:comprehensive|complete|detailed|thorough|in[- ]?depth|exhaustive)\s+(?:guide|overview|breakdown|summary|list|analysis|walkthrough|look)\b",
            Some((2.0, 1.0, 0.5)),
        ),
        mk(
            "sure_here_opener",
            0.80,
            r"(?i)^\s*(?:sure[!,.]?\s+|absolutely[!,.]?\s+|certainly[!,.]?\s+|of course[!,.]?\s+)?here'?s\s+(?:a|an|the|what|how|your)\b",
            Some((2.5, 0.0, 0.0)),
        ),
        mk(
            "article_self_reference",
            0.65,
            r"(?i)\b(?:in this (?:article|guide|post|tutorial|piece)|this (?:article|guide|post|tutorial) (?:will|covers|explores|examines|walks|aims))\b",
            Some((1.5, 1.0, 0.5)),
        ),
        // ── Question restatement ─────────────────────────────────────────────
        mk(
            "question_restatement",
            0.93,
            r"(?i)\b(?:you (?:asked|mentioned|wanted to know) about|regarding your (?:question|query|request)|to address your (?:question|query|concern)|in response to your)\b",
            Some((2.0, 1.0, 0.5)),
        ),
        // ── Self-identification ─────────────────────────────────────────────
        mk(
            "self_id_as_ai",
            0.99,
            r"(?i)\bas an?\s+(?:ai|artificial intelligence|language model|large language model|llm)\b",
            None,
        ),
        mk(
            "self_id_no_opinions",
            0.97,
            r"(?i)\bi (?:don'?t|do not) have (?:personal )?(?:opinions|feelings|beliefs|preferences)\b",
            None,
        ),
        mk(
            "self_id_training_data",
            0.98,
            r"(?i)\bbased on (?:my )?(?:training data|training cutoff|knowledge cutoff)\b",
            None,
        ),
        mk(
            "self_id_realtime",
            0.97,
            r"(?i)\bi (?:can'?t|cannot|don'?t) (?:browse|access) (?:the )?(?:internet|web)\b",
            None,
        ),
        // ── Closing pleasantries (cluster in the last 20%) ──────────────────
        mk(
            "hope_helps",
            0.70,
            r"(?i)\b(?:i )?hope (?:this|that|it) (?:helps|was helpful|is helpful|clarifies|clears (?:this|things) up|answers your)\b",
            Some((0.3, 0.6, 2.0)),
        ),
        mk(
            "feel_free_to",
            0.78,
            r"(?i)\b(?:please )?feel free to (?:ask|reach out|let me know|contact|drop)\b",
            Some((0.3, 0.6, 2.0)),
        ),
        mk(
            "dont_hesitate",
            0.85,
            r"(?i)\b(?:please )?don'?t hesitate to (?:ask|reach out|contact|get in touch)\b",
            Some((0.3, 0.6, 2.0)),
        ),
        mk(
            "let_me_know_if",
            0.82,
            r"(?i)\blet me know if (?:you (?:have any|need|'?d like|want)|there'?s anything|anything is unclear|that makes sense)\b",
            Some((0.3, 0.6, 2.0)),
        ),
        mk(
            "happy_to_further",
            0.78,
            r"(?i)\bhappy to (?:help|clarify|elaborate|explain|assist|walk you through)\b",
            Some((0.3, 0.6, 2.0)),
        ),
        // ── Bracket asides ──────────────────────────────────────────────────
        mk(
            "bracket_aside_note",
            0.90,
            r"(?i)\[(?:note|disclaimer|editor'?s? note|important|caveat|aside|context|background|fyi|tl;?dr|summary):\s+[^\]]{3,}\]",
            None,
        ),
        mk(
            "bracket_aside_image_desc",
            0.92,
            r"(?i)\[image:\s*(?:a|an|the)\s+[^\]]+\]",
            None,
        ),
        mk(
            "bracket_self_id",
            0.99,
            r"(?i)\[as an?\s+(?:ai|assistant|language model)[^\]]*\]",
            None,
        ),
        // ── Markdown envelopes ──────────────────────────────────────────────
        mk(
            "markdown_envelope_below",
            0.88,
            r"(?i)\b(?:below|here|what follows) is (?:a |an |the |my |your )?(?:draft|brief|comprehensive|summary|overview|outline|breakdown|attempt|version|response)\b",
            Some((2.0, 0.5, 0.0)),
        ),
        mk(
            "markdown_envelope_above",
            0.85,
            r"(?i)\b(?:above|the above) is (?:a |an |the )?(?:comprehensive|brief|complete|summary|overview)\b",
            Some((0.0, 0.5, 2.5)),
        ),
        // ── Hedge stacks ────────────────────────────────────────────────────
        mk(
            "hedge_important_to_note",
            0.82,
            r"(?i)\bit(?:'s| is) (?:important|worth|essential|crucial|critical|necessary) to (?:note|mention|understand|recognize|acknowledge|consider|highlight|emphasize|point out|remember)\b",
            None,
        ),
        mk(
            "hedge_should_be_noted",
            0.85,
            r"(?i)\bit should be (?:noted|emphasized|mentioned|stressed|highlighted) that\b",
            None,
        ),
        mk(
            "hedge_please_bear",
            0.75,
            r"(?i)\bplease (?:bear|keep) in mind\b",
            None,
        ),
        // ── Conclusion / summary markers (tail-weighted) ────────────────────
        mk(
            "conclusion_marker",
            0.78,
            r"(?i)\b(?:in conclusion|to conclude|in closing|in summary|to summarize|to sum up|all in all|at the end of the day|the bottom line is|when all is said and done)\b",
            Some((0.0, 0.5, 1.8)),
        ),
        mk(
            "sequencing_firstly",
            0.80,
            r"(?i)\b(?:firstly|secondly|thirdly|fourthly|fifthly|lastly)\b",
            None,
        ),
        mk(
            "restatement_marker",
            0.72,
            r"(?i)\b(?:in other words|simply put|put simply|to put it (?:simply|another way|plainly)|what (?:this|that|i) (?:means is|mean is)|in plain(?:er)? (?:english|terms))\b",
            None,
        ),
        // ── Sycophancy ──────────────────────────────────────────────────────
        mk(
            "sycophancy_great_question",
            0.95,
            r"(?i)\b(?:great|excellent|fantastic|wonderful|brilliant) question\b",
            None,
        ),
        // ── Cross-reference filler ──────────────────────────────────────────
        mk(
            "crossref_as_mentioned",
            0.80,
            r"(?i)\bas (?:mentioned|noted|discussed|stated|outlined|described) (?:above|earlier|before|previously)\b",
            None,
        ),
        // ── Journey / navigate ──────────────────────────────────────────────
        mk(
            "journey_navigate",
            0.95,
            r"(?i)\bas we (?:navigate|delve into|explore|embark on|journey through)\b",
            None,
        ),
        // ── Three-question opener ───────────────────────────────────────────
        mk(
            "three_question_opener",
            0.92,
            r"(?i)\b(?:you might be|you may be|you'?re probably) (?:wondering|asking yourself|thinking)\b",
            Some((2.0, 1.0, 0.5)),
        ),
    ]
});

pub fn extract(body_text: &str, headings: &[String]) -> Metatext {
    let total_words = body_text.split_whitespace().count().max(1);
    let mut signals: Vec<Signal> = Vec::new();
    let mut weighted_sum = 0.0_f64;

    for p in PATTERNS.iter() {
        for m in p.re.find_iter(body_text).take(10) {
            let word_offset = body_text[..m.start()].split_whitespace().count();
            let pos_pct = (word_offset as f64 / total_words as f64) * 100.0;
            let pos_w = match p.weights {
                None => 1.0,
                Some((h, mid, t)) => {
                    if pos_pct < 15.0 {
                        h
                    } else if pos_pct > 80.0 {
                        t
                    } else {
                        mid
                    }
                }
            };
            let snippet: String = m
                .as_str()
                .chars()
                .take(120)
                .collect::<String>()
                .trim()
                .replace('\n', " ");
            weighted_sum += p.confidence * pos_w;
            signals.push(Signal {
                kind: p.kind,
                confidence: p.confidence,
                snippet,
                position_pct: (pos_pct * 10.0).round() / 10.0,
            });
        }
    }

    let heading_skeleton = compute_heading_skeleton(headings);

    // Density: weighted hits per 1000 words.
    let density = (weighted_sum * 1000.0 / total_words as f64 * 100.0).round() / 100.0;

    // Verdict mixes density + skeleton + single-shot fatals.
    let has_fatal = signals.iter().any(|s| s.confidence >= 0.97);
    let strong_skeleton = heading_skeleton.jaccard >= 0.35 && heading_skeleton.matched.len() >= 4;

    let verdict = if has_fatal || strong_skeleton {
        "metatext_heavy"
    } else if density > 5.0 || (density > 1.5 && heading_skeleton.jaccard >= 0.20) {
        "metatext_heavy"
    } else if density > 1.5 {
        "suspicious"
    } else {
        "clean"
    };

    signals.truncate(40);

    Metatext {
        signals,
        heading_skeleton,
        weighted_score_per_1000_words: density,
        verdict,
    }
}

fn compute_heading_skeleton(headings: &[String]) -> HeadingSkeleton {
    let normalized: HashSet<String> = headings
        .iter()
        .map(|h| {
            h.trim()
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|h| !h.is_empty())
        .collect();
    let matched: Vec<String> = normalized
        .iter()
        .filter(|h| CANONICAL_HEADINGS.contains(h.as_str()))
        .cloned()
        .collect();
    let union = normalized.len() + CANONICAL_HEADINGS.len() - matched.len();
    let jaccard = if union == 0 {
        0.0
    } else {
        matched.len() as f64 / union as f64
    };
    HeadingSkeleton {
        jaccard: (jaccard * 1000.0).round() / 1000.0,
        matched,
    }
}

pub fn suggestion(m: &Metatext) -> Option<String> {
    match m.verdict {
        "clean" => None,
        "suspicious" => Some(format!(
            "Metatext density {:.2}/1000 words. {} signals. Check for process narration, hedges, closing pleasantries.",
            m.weighted_score_per_1000_words,
            m.signals.len()
        )),
        "metatext_heavy" => {
            if !m.heading_skeleton.matched.is_empty() {
                Some(format!(
                    "Metatext heavy: density {:.2}/1000 words + heading skeleton matches AI table-of-contents ({} of: {}). Rewrite without process narration.",
                    m.weighted_score_per_1000_words,
                    m.heading_skeleton.matched.len(),
                    m.heading_skeleton.matched.join(", ")
                ))
            } else {
                Some(format!(
                    "Metatext heavy: density {:.2}/1000 words, {} signals. Strong agent-speaking-not-content fingerprint.",
                    m.weighted_score_per_1000_words,
                    m.signals.len()
                ))
            }
        }
        _ => None,
    }
}

