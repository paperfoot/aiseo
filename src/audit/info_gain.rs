//! Information Gain proxy.
//!
//! Information Gain is an SEO-community frame (Indig / Search Engine Land),
//! not a Google-acknowledged ranking signal. The patent (US20200349181A1)
//! exists but Google has never confirmed it weighs ranking. This module
//! counts the deterministic, locally-detectable signals that proxy for
//! original first-party data — useful as a content-quality heuristic
//! regardless of whether any specific algorithm consumes it.
//!
//! Scoring uses the "5-to-7 rule" from Indig's follow-up writeups:
//! 0–2 unique signals reads as rewritten content; 5–7 competes; 8+ leads.
//! Score is a 0–10 integer surfaced on every audit.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

#[derive(Serialize)]
pub struct InformationGain {
    /// 0..10 — see scoring rule above. Hard cap at 10 for output stability.
    pub score: u32,
    /// Per-signal counts the score rolls up from.
    pub counts: SignalCounts,
    /// Verbatim matches, capped at 6 per kind for envelope size.
    pub samples: Vec<Sample>,
}

#[derive(Serialize)]
pub struct SignalCounts {
    /// Named-source quotation: `"..." — Name Surname` or `"..." (Name, ...)`.
    pub named_quotes: usize,
    /// First-party sample-size disclosure: `n=N`, `N patients`, `N participants`,
    /// `N samples`, `N respondents`, `N stores`, `N customers`, etc.
    pub sample_sizes: usize,
    /// Year-over-year deltas: `up X% from 2024`, `down X% vs last year`,
    /// percentage-point comparisons.
    pub yoy_deltas: usize,
    /// First-party authorship: `we analysed`, `our dataset`, `our study`,
    /// `we measured`, `our team`.
    pub first_person_evidence: usize,
    /// Method / methodology disclosure: `we used`, `methodology`,
    /// `methods`, `protocol`, `inclusion criteria`.
    pub method_disclosure: usize,
    /// Citation-style numbered references: `[1]`, `[12]`.
    pub numbered_citations: usize,
}

#[derive(Serialize)]
pub struct Sample {
    pub kind: &'static str,
    pub snippet: String,
}

static NAMED_QUOTE: Lazy<Regex> = Lazy::new(|| {
    // Curly or straight quotes of ≥20 chars, followed by an em-dash /
    // hyphen attribution OR a `(Name, source)` style attribution.
    Regex::new(
        r#"["“][^"”\n]{20,}["”]\s*[—–-]\s*[A-Z][a-z]+(?:\s+[A-Z][a-zA-Z\-']+)+"#,
    )
    .unwrap()
});

static SAMPLE_SIZE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(n\s*=\s*\d+|\d+(?:,\d{3})*\s+(patients?|participants?|respondents?|samples?|subjects?|stores?|customers?|transactions?|users?|companies|firms))\b",
    )
    .unwrap()
});

static YOY_DELTA: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(up|down|rose|fell|grew|declined|increased|decreased)\s+(by\s+)?\d+(\.\d+)?\s*(%|percentage\s+points?|pp)\s+(from|since|vs\.?|versus|compared\s+to|year[-\s]over[-\s]year|YoY)\b",
    )
    .unwrap()
});

static FIRST_PERSON_EVIDENCE: Lazy<Regex> = Lazy::new(|| {
    // First-person evidence patterns. Two tiers:
    //   - hard evidence verbs: analysed, measured, tested, surveyed,
    //     ran, conducted, sampled, interviewed, observed, sequenced,
    //     benchmarked, profiled, simulated, modelled, replicated
    //   - first-party action verbs: built, train(ed), develop(ed),
    //     designed, engineered, prototyped, deployed, ship(ped), launched,
    //     redirect, repair, treat — the voicey "we build / we redirect /
    //     we treat" pattern that founder pages and product sites use.
    //   - our-N: dataset, study, analysis, sample, cohort, lab,
    //     experiment, results, findings, team-found/measured/observed.
    Regex::new(
        r"(?i)\b(we\s+(analy[sz]ed|measured|tracked|tested|surveyed|ran|conducted|sampled|interviewed|observed|sequenced|benchmarked|profiled|simulated|modell?ed|replicated|built|build|trained?|train|develop(ed)?|designed|engineered|prototyped|deployed|ship(ped)?|launched|redirect|repair|treat)|our\s+(dataset|study|analysis|sample|cohort|lab|experiment|results|findings|patients|approach|team\s+(found|measured|observed|built|developed)))\b",
    )
    .unwrap()
});

static METHOD_DISCLOSURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(methodology|inclusion\s+criteria|exclusion\s+criteria|study\s+design|sample\s+size|control\s+group|double[-\s]blind|randomi[sz]ed|cross[-\s]sectional)\b",
    )
    .unwrap()
});

static NUMBERED_CITATION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\d{1,3}\]").unwrap());

pub fn extract(body_text: &str) -> InformationGain {
    let named_quotes = NAMED_QUOTE.find_iter(body_text).count();
    let sample_sizes = SAMPLE_SIZE.find_iter(body_text).count();
    let yoy_deltas = YOY_DELTA.find_iter(body_text).count();
    let first_person_evidence = FIRST_PERSON_EVIDENCE.find_iter(body_text).count();
    let method_disclosure = METHOD_DISCLOSURE.find_iter(body_text).count();
    let numbered_citations = NUMBERED_CITATION.find_iter(body_text).count();

    let total = named_quotes
        + sample_sizes
        + yoy_deltas
        + first_person_evidence
        + method_disclosure
        + numbered_citations.min(5); // numbered cites cap at 5 to stop one-page footnote-bombs

    let score = (total as u32).min(10);

    let mut samples: Vec<Sample> = Vec::new();
    push_samples(&mut samples, "named_quote", &NAMED_QUOTE, body_text, 3);
    push_samples(&mut samples, "sample_size", &SAMPLE_SIZE, body_text, 3);
    push_samples(&mut samples, "yoy_delta", &YOY_DELTA, body_text, 3);
    push_samples(
        &mut samples,
        "first_person_evidence",
        &FIRST_PERSON_EVIDENCE,
        body_text,
        3,
    );
    push_samples(&mut samples, "method_disclosure", &METHOD_DISCLOSURE, body_text, 3);

    InformationGain {
        score,
        counts: SignalCounts {
            named_quotes,
            sample_sizes,
            yoy_deltas,
            first_person_evidence,
            method_disclosure,
            numbered_citations,
        },
        samples,
    }
}

fn push_samples(
    out: &mut Vec<Sample>,
    kind: &'static str,
    re: &Regex,
    text: &str,
    max: usize,
) {
    for m in re.find_iter(text).take(max) {
        let snippet: String = m
            .as_str()
            .chars()
            .take(120)
            .collect::<String>()
            .trim()
            .replace('\n', " ");
        out.push(Sample {
            kind,
            snippet,
        });
    }
}

/// Suggestion text appended to the audit when the score is weak.
pub fn suggestion(ig: &InformationGain, word_count: usize) -> Option<String> {
    // Only relevant for pages long enough to plausibly carry evidence.
    if word_count < 300 {
        return None;
    }
    // Information Gain is an SEO-community frame (Indig / Search Engine Land),
    // not a Google-acknowledged ranking signal. The patent (US20200349181A1)
    // exists but Google has never confirmed it weighs ranking. Treat the
    // 5-to-7 rule as a content-quality heuristic, not a ranking guarantee.
    match ig.score {
        0..=1 => Some(format!(
            "Information Gain {}/10. Rewritten / templated content reads weak. Add named-source quotes, sample sizes (n=…), YoY deltas, first-party evidence.",
            ig.score
        )),
        2..=4 => Some(format!(
            "Information Gain {}/10. Below the competitive band (5..7). Add first-party data: named quotes, methodology, sample sizes.",
            ig.score
        )),
        _ => None,
    }
}
