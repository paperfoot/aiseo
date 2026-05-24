//! Convert raw findings into a flat list of actionable suggestions plus a
//! 0-100 score. The score is intentionally rough — agents care about the
//! suggestion list, humans want a quick gut number.

use super::{ContentStructure, Freshness, Meta, OpenGraph, PositionBias};
use serde::Serialize;

/// Per-component deduction. Agents read this to know *which* axis to fix
/// next, not just the bottom-line score.
#[derive(Serialize)]
pub struct ScoreBreakdown {
    pub total: u32,
    pub components: Vec<ScoreComponent>,
}

#[derive(Serialize)]
pub struct ScoreComponent {
    pub name: &'static str,
    pub deducted: u32,
    pub reason: &'static str,
}

pub fn build(
    meta: &Meta,
    og: &OpenGraph,
    content: &ContentStructure,
    pos: &PositionBias,
    fresh: &Freshness,
    schema_types: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // ── Metadata ────────────────────────────────────────────────────────────
    match meta.title.as_deref() {
        None => out.push("Title absent. Aim 50..60 chars.".into()),
        Some(t) if t.chars().count() < 30 => out.push(format!(
            "Title is {} chars. Aim 50..60.",
            t.chars().count()
        )),
        Some(t) if t.chars().count() > 70 => out.push(format!(
            "Title is {} chars. Search engines truncate past 60.",
            t.chars().count()
        )),
        _ => {}
    }

    match meta.description.as_deref() {
        None => out.push("Meta description absent. 150..160 chars.".into()),
        Some(d) if d.chars().count() < 100 => out.push(format!(
            "Meta description is {} chars. Aim 150..160.",
            d.chars().count()
        )),
        Some(d) if d.chars().count() > 170 => out.push(format!(
            "Meta description is {} chars. Snippets truncate past 160.",
            d.chars().count()
        )),
        _ => {}
    }

    if og.title.is_none() {
        out.push("og:title absent.".into());
    }
    if og.image.is_none() {
        out.push("og:image absent. 1200×630.".into());
    }

    // ── Content structure ────────────────────────────────────────────────────
    if content.h1.is_empty() {
        out.push("H1 absent.".into());
    } else if content.h1.len() > 1 {
        out.push(format!(
            "{} H1s. Use one; demote the rest.",
            content.h1.len()
        ));
    }
    if content.h2.len() < 2 {
        out.push(format!(
            "{} H2s. 3..5 helps passage-level retrieval.",
            content.h2.len()
        ));
    }
    if content.word_count < 300 {
        out.push(format!(
            "Body is {} words. 800..1500 tends to win on comprehensiveness.",
            content.word_count
        ));
    }
    if !content.has_tldr {
        out.push("No TL;DR. 40..60 words in the first 10%.".into());
    }
    if !content.has_credentials && content.has_author {
        out.push("Author has no credentials. MD, PhD, MSc lift ChatGPT and Claude citation.".into());
    }

    // ── Schema ───────────────────────────────────────────────────────────────
    if schema_types.is_empty() {
        out.push("No JSON-LD. At minimum: Article + Organization. FAQ for question pages.".into());
    }

    // ── Freshness ────────────────────────────────────────────────────────────
    if fresh.date_modified.is_none() {
        out.push("dateModified absent. Perplexity weights freshness.".into());
    } else if let Some(days) = fresh.days_since_modified
        && days > 90
    {
        out.push(format!(
            "Last modified {} days ago. Refresh for Perplexity and AI Mode visibility.",
            days
        ));
    }

    // ── Position bias ────────────────────────────────────────────────────────
    out.extend(pos.warnings.iter().cloned());

    out
}

/// Single source of truth for scoring. Both `score()` (which returns just
/// the total) and `score_breakdown()` (which returns per-component
/// deductions) call this — keeps them from drifting.
fn deductions(
    meta: &Meta,
    og: &OpenGraph,
    content: &ContentStructure,
    fresh: &Freshness,
    schema_types: &[String],
) -> Vec<ScoreComponent> {
    let mut out: Vec<ScoreComponent> = Vec::new();
    if meta.title.is_none() {
        out.push(ScoreComponent {
            name: "meta_title",
            deducted: 15,
            reason: "Missing <title>",
        });
    }
    if meta.description.is_none() {
        out.push(ScoreComponent {
            name: "meta_description",
            deducted: 10,
            reason: "Missing meta description",
        });
    }
    if og.title.is_none() {
        out.push(ScoreComponent {
            name: "og_title",
            deducted: 5,
            reason: "Missing og:title",
        });
    }
    if og.image.is_none() {
        out.push(ScoreComponent {
            name: "og_image",
            deducted: 10,
            reason: "Missing og:image",
        });
    }
    if content.h1.is_empty() {
        out.push(ScoreComponent {
            name: "h1",
            deducted: 10,
            reason: "No H1 heading",
        });
    }
    if content.h2.len() < 2 {
        out.push(ScoreComponent {
            name: "h2_count",
            deducted: 5,
            reason: "Fewer than 2 H2 headings",
        });
    }
    if content.word_count < 300 {
        out.push(ScoreComponent {
            name: "word_count",
            deducted: 10,
            reason: "Body under 300 words",
        });
    }
    if !content.has_tldr {
        out.push(ScoreComponent {
            name: "tldr",
            deducted: 5,
            reason: "No TL;DR detected",
        });
    }
    if schema_types.is_empty() {
        out.push(ScoreComponent {
            name: "schema",
            deducted: 15,
            reason: "No JSON-LD schema",
        });
    }
    if fresh.date_modified.is_none() {
        out.push(ScoreComponent {
            name: "date_modified",
            deducted: 5,
            reason: "Missing dateModified",
        });
    }
    out
}

pub fn score_breakdown(
    meta: &Meta,
    og: &OpenGraph,
    content: &ContentStructure,
    fresh: &Freshness,
    schema_types: &[String],
) -> ScoreBreakdown {
    let components = deductions(meta, og, content, fresh, schema_types);
    let total_deducted: u32 = components.iter().map(|c| c.deducted).sum();
    let total = 100u32.saturating_sub(total_deducted);
    ScoreBreakdown { total, components }
}


