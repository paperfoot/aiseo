//! Audit engine. Reads an HTML or Markdown file and returns a structured
//! report covering metadata, content structure, position bias, freshness,
//! and a list of suggestions. The Python skill at
//! `~/.claude/skills/seo-geo-optimizer` does the same thing in 13 separate
//! scripts; this module is the consolidated Rust port.

use serde::Serialize;
use std::path::Path;

mod ai_slop;
mod content;
mod copy_precision;
mod design_slop;
mod entities;
mod evidence;
pub mod factors;
mod freshness;
mod info_gain;
mod keywords;
mod meta;
mod metatext;
mod position;
pub mod report;
mod suggest;
mod voice;

pub use ai_slop::AiSlop;
pub use content::{ContentStructure, HeadingOrderEntry, NoscriptKind};
pub use copy_precision::CopyPrecision;
pub use design_slop::DesignSlop;
pub use entities::Entities;
pub use evidence::Evidence;
pub use freshness::Freshness;
pub use info_gain::InformationGain;
pub use keywords::Keywords;
pub use meta::{Meta, OpenGraph, TwitterCard};
pub use metatext::Metatext;
pub use position::PositionBias;
pub use suggest::ScoreBreakdown;
pub use voice::Voice;

#[derive(Serialize)]
pub struct AuditReport {
    pub file: String,
    pub file_type: &'static str,
    pub meta: Meta,
    pub open_graph: OpenGraph,
    pub twitter_card: TwitterCard,
    pub schema_types: Vec<String>,
    pub content: ContentStructure,
    pub keywords: Keywords,
    pub entities: Entities,
    pub evidence: Evidence,
    pub voice: Voice,
    pub position_bias: PositionBias,
    pub freshness: Freshness,
    pub ai_slop: AiSlop,
    pub information_gain: InformationGain,
    pub metatext: Metatext,
    pub copy_precision: CopyPrecision,
    pub design_slop: DesignSlop,
    pub score: u32,
    pub score_breakdown: ScoreBreakdown,
    pub suggestions: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("unsupported file type: {0} (expected .html, .htm, .md, .mdx)")]
    UnsupportedType(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy)]
pub enum ContentType {
    Html,
    Markdown,
}

impl ContentType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
        }
    }

    /// Sniff the content type from the first few non-whitespace characters.
    /// HTML if it starts with `<` (handles `<!DOCTYPE`, `<html`, `<div`,
    /// bare `<meta>`, etc.); Markdown otherwise. Good enough for piped
    /// agent input — no need for libmagic.
    pub fn sniff(raw: &str) -> Self {
        let head = raw.trim_start();
        if head.starts_with('<') {
            Self::Html
        } else {
            Self::Markdown
        }
    }
}

pub fn audit_file(path: &Path) -> Result<AuditReport, AuditError> {
    if !path.exists() {
        return Err(AuditError::NotFound(path.display().to_string()));
    }
    let raw = std::fs::read_to_string(path)?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let content_type = match ext.as_str() {
        "html" | "htm" => ContentType::Html,
        "md" | "mdx" => ContentType::Markdown,
        other => {
            return Err(AuditError::UnsupportedType(other.to_string()));
        }
    };

    audit_content(raw, content_type, path.display().to_string())
}

/// Audit already-loaded content. Used by `audit_file` and by the stdin
/// path (`aiseo audit -`). `label` is what appears in the `file` field
/// of the report — a path, a URL, or "<stdin>".
pub fn audit_content(
    raw: String,
    content_type: ContentType,
    label: String,
) -> Result<AuditReport, AuditError> {
    let html_like = match content_type {
        ContentType::Html => raw.clone(),
        ContentType::Markdown => markdown_to_html_lite(&raw),
    };

    let doc = scraper::Html::parse_document(&html_like);
    let meta = meta::extract(&doc);
    let og = meta::extract_open_graph(&doc);
    let tw = meta::extract_twitter_card(&doc);
    let schema_types = meta::extract_schema_types(&doc);
    let content = content::extract(&doc);
    let position_bias = position::analyze(&content.body_text);
    let freshness = freshness::analyze(&html_like, &schema_types);
    let keywords = keywords::extract(&content.body_text);
    let entities = entities::extract(&content.body_text);
    let evidence = evidence::extract(&content.body_text);
    let voice = voice::extract(&content.body_text, &schema_types, &html_like);
    let ai_slop = ai_slop::extract(&content.body_text);
    let information_gain = info_gain::extract(&content.body_text);
    // metatext reads body text + heading order so it can compute the
    // canonical-AI-skeleton Jaccard alongside the lexical patterns.
    let heading_strings: Vec<String> = content
        .headings_in_order
        .iter()
        .map(|h| h.text.clone())
        .collect();
    let metatext = metatext::extract(&content.body_text, &heading_strings);
    let copy_precision = copy_precision::extract(&content.body_text);
    // design_slop reads the *raw HTML* (not the body text) so it sees CSS,
    // class strings, and inline styles.
    let design_slop = design_slop::extract(&html_like);

    let mut suggestions = suggest::build(
        &meta,
        &og,
        &content,
        &position_bias,
        &freshness,
        &schema_types,
    );
    if let Some(s) = ai_slop::suggestion(&ai_slop) {
        suggestions.push(s);
    }
    if let Some(s) = info_gain::suggestion(&information_gain, content.word_count) {
        suggestions.push(s);
    }
    if let Some(s) = metatext::suggestion(&metatext) {
        suggestions.push(s);
    }
    if let Some(s) = copy_precision::suggestion(&copy_precision) {
        suggestions.push(s);
    }
    if let Some(s) = design_slop::suggestion(&design_slop) {
        suggestions.push(s);
    }
    let score_breakdown = suggest::score_breakdown(
        &meta,
        &og,
        &content,
        &position_bias,
        &freshness,
        &ai_slop,
        &information_gain,
        &schema_types,
    );
    let score = score_breakdown.total;

    Ok(AuditReport {
        file: label,
        file_type: content_type.label(),
        meta,
        open_graph: og,
        twitter_card: tw,
        schema_types,
        content,
        keywords,
        entities,
        evidence,
        voice,
        position_bias,
        freshness,
        ai_slop,
        information_gain,
        metatext,
        copy_precision,
        design_slop,
        score,
        score_breakdown,
        suggestions,
    })
}

/// Lightweight Markdown → HTML for audit purposes. We don't need full
/// rendering — just enough that the scraper-based extractors find headings,
/// paragraphs, and text content. The audit isn't testing your Markdown
/// renderer; it's testing your SEO/GEO surface.
fn markdown_to_html_lite(md: &str) -> String {
    // Strip YAML frontmatter if present.
    let body = if md.starts_with("---") {
        if let Some(end) = md[3..].find("\n---") {
            &md[end + 4 + 3..]
        } else {
            md
        }
    } else {
        md
    };

    let mut out = String::with_capacity(body.len() + 256);
    out.push_str("<!DOCTYPE html><html><head></head><body>");

    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push_str(&format!("<h3>{}</h3>", html_escape(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push_str(&format!("<h2>{}</h2>", html_escape(rest)));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push_str(&format!("<h1>{}</h1>", html_escape(rest)));
        } else if !trimmed.is_empty() {
            out.push_str(&format!("<p>{}</p>", html_escape(trimmed)));
        }
    }

    out.push_str("</body></html>");
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
