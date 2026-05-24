//! Audit engine. Reads an HTML or Markdown file and returns a structured
//! report covering metadata, content structure, position bias, freshness,
//! and a list of suggestions. The Python skill at
//! `~/.claude/skills/seo-geo-optimizer` does the same thing in 13 separate
//! scripts; this module is the consolidated Rust port.

use serde::Serialize;
use std::path::Path;

mod content;
mod entities;
mod evidence;
pub mod factors;
mod freshness;
mod keywords;
mod meta;
mod position;
pub mod report;
mod suggest;
mod voice;

pub use content::ContentStructure;
pub use entities::Entities;
pub use evidence::Evidence;
pub use freshness::Freshness;
pub use keywords::Keywords;
pub use meta::{Meta, OpenGraph, TwitterCard};
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

    let (file_type, html_like) = match ext.as_str() {
        "html" | "htm" => ("html", raw.clone()),
        "md" | "mdx" => ("markdown", markdown_to_html_lite(&raw)),
        other => {
            return Err(AuditError::UnsupportedType(other.to_string()));
        }
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

    let suggestions = suggest::build(&meta, &og, &content, &position_bias, &freshness, &schema_types);
    let score_breakdown = suggest::score_breakdown(&meta, &og, &content, &freshness, &schema_types);
    let score = score_breakdown.total;

    Ok(AuditReport {
        file: path.display().to_string(),
        file_type,
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
