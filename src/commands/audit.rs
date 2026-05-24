use serde::Serialize;
use std::path::PathBuf;

use crate::audit::factors as audit_factors;
use crate::audit::report::{self, ReportFormat, ReportInput};
use crate::audit::{self, AuditError};
use crate::error::AppError;
use crate::output::{self, Ctx};

#[derive(Serialize)]
struct AuditEnvelope {
    file: String,
    file_type: &'static str,
    score: u32,
    score_breakdown: audit::ScoreBreakdown,
    meta: audit::Meta,
    open_graph: audit::OpenGraph,
    twitter_card: audit::TwitterCard,
    schema_types: Vec<String>,
    content: ContentSummary,
    keywords: audit::Keywords,
    entities: audit::Entities,
    evidence: audit::Evidence,
    voice: audit::Voice,
    position_bias: audit::PositionBias,
    freshness: audit::Freshness,
    suggestions: Vec<String>,
}

#[derive(Serialize)]
struct ContentSummary {
    word_count: usize,
    h1: Vec<String>,
    h2: Vec<String>,
    h3: Vec<String>,
    has_tldr: bool,
    has_faq: bool,
    has_author: bool,
    has_credentials: bool,
}

pub fn run(
    ctx: Ctx,
    path: PathBuf,
    fail_under: Option<u32>,
    out: Option<PathBuf>,
    factors: Option<String>,
) -> Result<(), AppError> {
    let factor_list = match factors.as_deref() {
        Some(s) => audit_factors::parse_list(s).map_err(AppError::InvalidInput)?,
        None => Vec::new(),
    };

    let mut report = audit::audit_file(&path).map_err(|e| match e {
        AuditError::NotFound(p) => AppError::InvalidInput(format!("file not found: {p}")),
        AuditError::UnsupportedType(t) => AppError::InvalidInput(format!(
            "unsupported file type: .{t} (expected .html, .htm, .md, .mdx)"
        )),
        AuditError::Io(e) => AppError::Io(e),
    })?;

    let score = report.score;

    if !factor_list.is_empty() {
        report.suggestions =
            audit_factors::filter_suggestions(std::mem::take(&mut report.suggestions), &factor_list);
        report.score_breakdown.components = audit_factors::filter_components(
            std::mem::take(&mut report.score_breakdown.components),
            &factor_list,
        );
    }

    let envelope = AuditEnvelope {
        file: report.file,
        file_type: report.file_type,
        score: report.score,
        score_breakdown: report.score_breakdown,
        meta: report.meta,
        open_graph: report.open_graph,
        twitter_card: report.twitter_card,
        schema_types: report.schema_types,
        content: ContentSummary {
            word_count: report.content.word_count,
            h1: report.content.h1,
            h2: report.content.h2,
            h3: report.content.h3,
            has_tldr: report.content.has_tldr,
            has_faq: report.content.has_faq,
            has_author: report.content.has_author,
            has_credentials: report.content.has_credentials,
        },
        keywords: report.keywords,
        entities: report.entities,
        evidence: report.evidence,
        voice: report.voice,
        position_bias: report.position_bias,
        freshness: report.freshness,
        suggestions: report.suggestions,
    };

    if let Some(out_path) = out.as_ref() {
        let format = ReportFormat::from_extension(out_path)?;
        report::write(
            format,
            out_path,
            &ReportInput {
                envelope: &envelope,
                file_label: &envelope.file,
                score,
                suggestions: &envelope.suggestions,
            },
        )?;
        // With --out, stdout stays terse: one JSON or one line, never the
        // full envelope (the file is the deliverable).
        output::print_success_or(
            ctx,
            &serde_json::json!({
                "wrote": out_path.display().to_string(),
                "score": score,
            }),
            |_d| {
                use owo_colors::OwoColorize;
                println!(
                    "{} {} ({}/100)",
                    "Wrote".green(),
                    out_path.display(),
                    score
                );
            },
        );
    } else {
        output::print_success_or(ctx, &envelope, |e| {
            use owo_colors::OwoColorize;
            println!("{} {}", "Audit".bold(), e.file.dimmed());
            println!(
                "  score: {}/100   words: {}   schemas: {}",
                score_colour(e.score),
                e.content.word_count,
                if e.schema_types.is_empty() {
                    "none".red().to_string()
                } else {
                    e.schema_types.join(", ").green().to_string()
                }
            );
            if e.suggestions.is_empty() {
                println!("\n  {}", "No suggestions — ship it.".green());
            } else {
                println!("\n  Suggestions:");
                for s in &e.suggestions {
                    println!("   • {s}");
                }
            }
        });
    }

    // Quality gate. The audit JSON / file is already produced; this only
    // flips the exit code so CI / agents can branch on pass / fail.
    if let Some(threshold) = fail_under
        && score < threshold
    {
        return Err(AppError::QualityGate { score, threshold });
    }

    Ok(())
}

fn score_colour(score: u32) -> String {
    use owo_colors::OwoColorize;
    let s = format!("{score}");
    match score {
        90..=100 => s.green().to_string(),
        70..=89 => s.yellow().to_string(),
        _ => s.red().to_string(),
    }
}
