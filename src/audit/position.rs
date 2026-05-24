//! Position-bias analysis. 2026 measurements (iPullRank, AIBoost) put ~44%
//! of AI-search citations on the first ~30% of a page. Flag when high-leverage
//! signals — TL;DR, first statistic, first credential mention — sit below
//! that mark.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

static TLDR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)TL;?DR:?\s*").unwrap());
static STAT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(\d+(?:,\d{3})*(?:\.\d+)?)\s*(%|percent|years?|months?|patients?|people|users?|cases?)",
    )
    .unwrap()
});
static CREDENTIAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(MD|PhD|Ph\.D\.|M\.D\.|MBA|MSc|MPH|DDS|DMD|JD|RN|DO|DPM|OD|PharmD|DVM|EdD|PsyD)\b")
        .unwrap()
});

#[derive(Serialize)]
pub struct PositionBias {
    pub total_words: usize,
    pub tldr_position_pct: Option<f64>,
    pub first_stat_position_pct: Option<f64>,
    pub first_credential_position_pct: Option<f64>,
    pub warnings: Vec<String>,
}

pub fn analyze(body_text: &str) -> PositionBias {
    let words: Vec<&str> = body_text.split_whitespace().collect();
    let total = words.len();

    if total == 0 {
        return PositionBias {
            total_words: 0,
            tldr_position_pct: None,
            first_stat_position_pct: None,
            first_credential_position_pct: None,
            warnings: Vec::new(),
        };
    }

    let tldr_off = word_offset(body_text, &TLDR_RE);
    let stat_off = word_offset(body_text, &STAT_RE);
    let cred_off = word_offset(body_text, &CREDENTIAL_RE);

    let pct = |off: Option<usize>| -> Option<f64> {
        off.map(|o| ((o as f64 / total as f64) * 1000.0).round() / 10.0)
    };

    let tldr_pct = pct(tldr_off);
    let stat_pct = pct(stat_off);
    let cred_pct = pct(cred_off);

    let mut warnings = Vec::new();
    if let Some(p) = tldr_pct
        && p > 10.0
    {
        warnings.push(format!(
            "TL;DR sits at {p}% of body. Move into the first 10%."
        ));
    }
    if let Some(p) = stat_pct {
        if p > 30.0 {
            warnings.push(format!(
                "First statistic at {p}% of body. Front-load citation-worthy numbers."
            ));
        }
    } else if total >= 200 {
        warnings.push(
            "No statistics detected. Named numerical claims lift AI citation.".to_string(),
        );
    }

    PositionBias {
        total_words: total,
        tldr_position_pct: tldr_pct,
        first_stat_position_pct: stat_pct,
        first_credential_position_pct: cred_pct,
        warnings,
    }
}

fn word_offset(text: &str, re: &Regex) -> Option<usize> {
    let m = re.find(text)?;
    Some(text[..m.start()].split_whitespace().count())
}
