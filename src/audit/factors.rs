//! Tag each score component with a higher-level factor name so `--factors`
//! can let an agent re-audit specific areas after a partial fix.
//!
//! Mapping is small on purpose — we want fewer, intuitive buckets, not
//! one bucket per score component.

use super::suggest::ScoreComponent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Factor {
    Meta,
    Og,
    Content,
    Schema,
    Freshness,
    Position,
}

impl Factor {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "meta" => Some(Self::Meta),
            "og" | "open_graph" | "opengraph" => Some(Self::Og),
            "content" => Some(Self::Content),
            "schema" => Some(Self::Schema),
            "freshness" | "fresh" => Some(Self::Freshness),
            "position" | "position_bias" => Some(Self::Position),
            _ => None,
        }
    }

}

pub fn parse_list(raw: &str) -> Result<Vec<Factor>, String> {
    let mut out = Vec::new();
    for piece in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match Factor::parse(piece) {
            Some(f) => out.push(f),
            None => {
                return Err(format!(
                    "unknown factor `{piece}` (valid: meta, og, content, schema, freshness, position)"
                ));
            }
        }
    }
    Ok(out)
}

pub fn component_factor(name: &str) -> Factor {
    match name {
        "meta_title" | "meta_description" => Factor::Meta,
        "og_title" | "og_image" => Factor::Og,
        "h1" | "h2_count" | "word_count" | "tldr" => Factor::Content,
        "schema" => Factor::Schema,
        "date_modified" => Factor::Freshness,
        _ => Factor::Content,
    }
}

/// Drop suggestions whose factor isn't in the allow-list. We tag by best-
/// effort substring matching against the suggestion text — the suggestion
/// strings are already worded around the same vocabulary the factor names
/// use, so this is cheaper than a full classifier and good enough.
pub fn filter_suggestions(suggestions: Vec<String>, factors: &[Factor]) -> Vec<String> {
    if factors.is_empty() {
        return suggestions;
    }
    suggestions
        .into_iter()
        .filter(|s| factors.iter().any(|f| suggestion_matches(s, *f)))
        .collect()
}

fn suggestion_matches(s: &str, f: Factor) -> bool {
    let lower = s.to_ascii_lowercase();
    match f {
        Factor::Meta => lower.contains("meta ") || lower.contains("<title>") || lower.contains("title is"),
        Factor::Og => lower.contains("og:") || lower.contains("og_") || lower.contains("open graph"),
        Factor::Content => {
            lower.contains("body ")
                || lower.contains("h1")
                || lower.contains("h2")
                || lower.contains("tl;dr")
                || lower.contains("words")
                || lower.contains("heading")
        }
        Factor::Schema => lower.contains("schema") || lower.contains("json-ld"),
        Factor::Freshness => lower.contains("datemodified") || lower.contains("modified") || lower.contains("days ago"),
        Factor::Position => {
            lower.contains("tl;dr appears")
                || lower.contains("first statistic")
                || lower.contains("position")
        }
    }
}

pub fn filter_components(
    components: Vec<ScoreComponent>,
    factors: &[Factor],
) -> Vec<ScoreComponent> {
    if factors.is_empty() {
        return components;
    }
    components
        .into_iter()
        .filter(|c| factors.contains(&component_factor(c.name)))
        .collect()
}
