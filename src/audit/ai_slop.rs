//! AI / LLM slop detection.
//!
//! Deterministic, regex-only. No model, no API. Pattern bank drawn from
//! awnist/slop-cop, sam-paech/slop-score, peakoss/anti-slop, plus the
//! Ozigi production validator.
//!
//! Em-dashes are intentionally NOT flagged. Modern human writers use them
//! (especially in British English) and slop-score explicitly removed them
//! as a signal because "basically every model uses one in every response"
//! — zero discriminative power. Density of *other* tells is what carries.
//!
//! Verdict comes from confidence-weighted density per 1000 words:
//!   clean       < 3.0
//!   suspicious  3.0 .. 8.0
//!   likely_ai   > 8.0

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

#[derive(Serialize)]
pub struct AiSlop {
    pub signals: Vec<Signal>,
    pub density_per_1000_words: f64,
    pub verdict: &'static str,
}

#[derive(Serialize, Clone)]
pub struct Signal {
    pub kind: &'static str,
    pub confidence: &'static str,
    pub snippet: String,
    pub position_pct: f64,
}

struct Pattern {
    kind: &'static str,
    confidence: &'static str,
    re: Regex,
}

fn confidence_weight(c: &str) -> f64 {
    match c {
        "high" => 1.0,
        "medium" => 0.6,
        _ => 0.3,
    }
}

static PATTERNS: Lazy<Vec<Pattern>> = Lazy::new(|| {
    let mk = |kind, conf, p: &str| Pattern {
        kind,
        confidence: conf,
        re: Regex::new(p).unwrap_or_else(|e| panic!("bad regex for {kind}: {e}")),
    };
    vec![
        // "not X but Y" rhetorical pivot — common in legitimate prose
        // (Scott Alexander, Brian Potter, every essay tradition). Demoted
        // to low confidence per v0.6 stress test so it stops dominating
        // the slop score on essayistic writing.
        mk(
            "negation_pivot_but",
            "low",
            r"(?i)\b(not|don'?t|doesn'?t|isn'?t|aren'?t|never)\b[^.!?\n]{3,80},?\s+but\b",
        ),
        mk(
            "negation_pivot_em_dash",
            "high",
            r"(?i)\b(not|never)\b[^.!?\n—–]{3,60}[—–]\s*\w+",
        ),
        // "delve" stays high; "leverage/foster/underscore" demoted to
        // medium per Codex review — all three appear in legitimate
        // business / academic writing.
        mk("delve_only", "high", r"(?i)\bdelv(e|es|ed|ing)\b"),
        // Drop bare "foster" because it matches the surname (Foster
        // Wallace, Jodie Foster, surnamed authors generally). Keep the
        // verb forms "fosters / fostered / fostering" which are AI-slop
        // tells; the bare lemma "foster" is too noisy.
        mk(
            "leverage_foster_underscore",
            "medium",
            r"(?i)\b(leverag(e|es|ed|ing)|foster(s|ed|ing)|underscor(e|es|ed|ing))\b",
        ),
        // Tapestry / paradigm / multifaceted stay high (rare in real prose).
        // "nuanced" and "interplay" demoted — common in academic writing.
        mk(
            "tapestry_family",
            "high",
            r"(?i)\b(tapestry|multifaceted|paradigm|pivotal|intricate|intricacies|burgeoning)\b",
        ),
        mk(
            "nuanced_interplay",
            "low",
            r"(?i)\b(nuanced|interplay)\b",
        ),
        mk(
            "realm_landscape",
            "high",
            r"(?i)\bin\s+the\s+(realm|landscape|world|sphere|domain)\s+of\b",
        ),
        mk(
            "era_opener",
            "high",
            r"(?i)\bin\s+an?\s+era\s+(of|where|when|in\s+which)\b",
        ),
        mk(
            "today_fast_paced",
            "high",
            r"(?i)\bin\s+today'?s\s+(fast.?paced|digital|interconnected|ever.?(changing|evolving))\b",
        ),
        mk(
            "ever_evolving",
            "high",
            r"(?i)\b(ever.?(evolving|changing|growing|expanding)|rapidly\s+evolving)\b",
        ),
        mk(
            "testament_to",
            "high",
            r"(?i)\b(a|stands?\s+as\s+a)\s+testament\s+to\b",
        ),
        mk(
            "it_worth_noting",
            "high",
            r"(?i)\bit('?s|\s+is)\s+(worth\s+noting|important\s+to\s+note)\b|\bit\s+should\s+be\s+noted\b",
        ),
        mk(
            "navigate_complexities",
            "high",
            r"(?i)\bnavigat(e|ing|es|ed)\s+the\s+(complex(it(y|ies))?|landscape|intricacies|challenges|nuances)\b",
        ),
        // Per Codex review: single `**Label:** X` is normal markdown; only
        // becomes a signal at density. Drop to medium.
        mk("bold_colon_header", "medium", r"\*\*[^*\n]{1,40}:\*\*\s+\S"),
        // "overall" alone is too common. Keep the rest at high.
        mk(
            "false_conclusion_opener",
            "high",
            r"(?im)(^|\n)\s*(in\s+conclusion|in\s+summary|to\s+summari[sz]e|to\s+conclude|in\s+closing|all\s+in\s+all)[,\s]",
        ),
        mk(
            "not_only_but_also",
            "high",
            r"(?i)\bnot\s+only\b[^.!?\n]{3,80}\bbut\s+also\b",
        ),
        mk(
            "unlock_potential",
            "high",
            r"(?i)\b(unlock|harness|unleash)\s+(the\s+)?(power|potential|magic|true\s+\w+)\b",
        ),
        mk(
            "paramount_importance",
            "high",
            r"(?i)\b(of\s+paramount\s+importance|of\s+utmost\s+importance|cannot\s+be\s+overstated)\b",
        ),
        mk(
            "seamlessly_integrate",
            "high",
            r"(?i)\bseamlessly\s+(integrat\w+|combin\w+|blend\w+|merge\w+|transition\w+)\b",
        ),
        mk(
            "world_of_x",
            "high",
            r"(?i)\b(welcome\s+to|enter)\s+the\s+world\s+of\b",
        ),
        mk(
            "hedge_stack",
            "high",
            r"(?i)\b(perhaps|arguably|seemingly|possibly|potentially|presumably)\b[^.!?\n]{0,80}\b(perhaps|arguably|seemingly|might|could|may|possibly|potentially|presumably)\b",
        ),
        mk(
            "furthermore_moreover",
            "medium",
            r"(?i)\b(furthermore|moreover|additionally)\b",
        ),
    ]
});

pub fn extract(body_text: &str) -> AiSlop {
    let words = body_text.split_whitespace().count();
    let total_words = words.max(1) as f64;

    let mut signals: Vec<Signal> = Vec::new();
    let mut weighted: f64 = 0.0;

    for p in PATTERNS.iter() {
        for m in p.re.find_iter(body_text).take(20) {
            let pos = body_text[..m.start()].split_whitespace().count() as f64;
            let position_pct = ((pos / total_words) * 1000.0).round() / 10.0;
            let snippet: String = m
                .as_str()
                .chars()
                .take(120)
                .collect::<String>()
                .trim()
                .replace('\n', " ");
            weighted += confidence_weight(p.confidence);
            signals.push(Signal {
                kind: p.kind,
                confidence: p.confidence,
                snippet,
                position_pct,
            });
        }
    }

    let density = (weighted / total_words) * 1000.0;
    let density = (density * 100.0).round() / 100.0;

    let verdict = if density < 3.0 {
        "clean"
    } else if density < 8.0 {
        "suspicious"
    } else {
        "likely_ai"
    };

    // Cap surfaced signals so the JSON envelope stays readable for big pages.
    signals.truncate(40);

    AiSlop {
        signals,
        density_per_1000_words: density,
        verdict,
    }
}

/// Suggestion text the audit pipeline appends when the verdict isn't clean.
pub fn suggestion(slop: &AiSlop) -> Option<String> {
    match slop.verdict {
        "clean" => None,
        "suspicious" => Some(format!(
            "AI-slop density {:.1}/1000 words. {} signals. Review for tells like \"delve\", \"tapestry\", negation pivots.",
            slop.density_per_1000_words,
            slop.signals.len()
        )),
        "likely_ai" => Some(format!(
            "AI-slop density {:.1}/1000 words ({} signals). Heavy LLM-writing fingerprint. Rewrite the worst sections.",
            slop.density_per_1000_words,
            slop.signals.len()
        )),
        _ => None,
    }
}
