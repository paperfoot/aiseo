//! Fetch a live URL, run it through the audit pipeline, and return the
//! same envelope as `audit` plus a `fetched` metadata block.
//!
//! Owns the network concerns — timeout, redirects, user-agent — so the
//! plain `audit` command stays pure-filesystem.

use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use crate::audit::factors as audit_factors;
use crate::audit::report::{self, ReportFormat, ReportInput};
use crate::audit::{self, AuditError, ContentType};
use crate::error::AppError;
use crate::output::{self, Ctx};

const USER_AGENT: &str = concat!("aiseo/", env!("CARGO_PKG_VERSION"), " (+https://github.com/paperfoot/aiseo)");

#[derive(Serialize)]
struct FetchEnvelope {
    fetched: FetchInfo,
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
    ai_slop: audit::AiSlop,
    information_gain: audit::InformationGain,
    metatext: audit::Metatext,
    copy_precision: audit::CopyPrecision,
    design_slop: audit::DesignSlop,
    suggestions: Vec<String>,
}

#[derive(Serialize)]
struct FetchInfo {
    url: String,
    status: u16,
    content_type: Option<String>,
    bytes: usize,
    fetched_at: String,
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
    image_count: usize,
    missing_alt_count: usize,
    html_lang: Option<String>,
    headings_in_order: Vec<audit::HeadingOrderEntry>,
    hreflangs: Vec<String>,
    noscript_kind: audit::NoscriptKind,
}

pub fn run(
    ctx: Ctx,
    url: String,
    fail_under: Option<u32>,
    out: Option<PathBuf>,
    factors: Option<String>,
) -> Result<(), AppError> {
    let factor_list = match factors.as_deref() {
        Some(s) => audit_factors::parse_list(s).map_err(AppError::InvalidInput)?,
        None => Vec::new(),
    };

    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::InvalidInput(format!(
            "URL must start with http:// or https://, got `{url}`"
        )));
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .redirects(8)
        .build();

    let response = agent.get(&url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => AppError::Transient(format!(
            "HTTP {code} fetching {url}"
        )),
        ureq::Error::Transport(t) => AppError::Transient(format!("network error: {t}")),
    })?;

    let status = response.status();
    let content_type = response.header("content-type").map(str::to_string);

    // Cap the body at 10 MB so a hostile / accidental huge response can't
    // exhaust memory. Reads only up to the limit; truncation is signalled
    // via the `bytes` field.
    const MAX_BYTES: u64 = 10 * 1024 * 1024;
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    response
        .into_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut buf)
        .map_err(|e| AppError::Transient(format!("reading body: {e}")))?;
    let body = String::from_utf8_lossy(&buf).into_owned();
    let bytes = body.len();

    let ctype = match content_type.as_deref() {
        Some(ct) if ct.contains("markdown") => ContentType::Markdown,
        _ => ContentType::Html,
    };

    // Audit in memory; the URL is the file label so the envelope is stable
    // across runs and never leaks the tempdir path.
    let mut report = audit::audit_content(body, ctype, url.clone()).map_err(|e| match e {
        AuditError::NotFound(p) => AppError::InvalidInput(format!("file not found: {p}")),
        AuditError::UnsupportedType(t) => AppError::InvalidInput(format!(
            "unsupported file type: .{t}"
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
    let envelope = FetchEnvelope {
        fetched: FetchInfo {
            url: url.clone(),
            status,
            content_type,
            bytes,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        },
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
            image_count: report.content.image_count,
            missing_alt_count: report.content.missing_alt_count,
            html_lang: report.content.html_lang,
            headings_in_order: report.content.headings_in_order,
            hreflangs: report.content.hreflangs,
            noscript_kind: report.content.noscript_kind,
        },
        keywords: report.keywords,
        entities: report.entities,
        evidence: report.evidence,
        voice: report.voice,
        position_bias: report.position_bias,
        freshness: report.freshness,
        ai_slop: report.ai_slop,
        information_gain: report.information_gain,
        metatext: report.metatext,
        copy_precision: report.copy_precision,
        design_slop: report.design_slop,
        suggestions: report.suggestions,
    };

    if let Some(out_path) = out.as_ref() {
        let format = ReportFormat::from_extension(out_path)?;
        report::write(
            format,
            out_path,
            &ReportInput {
                envelope: &envelope,
                file_label: &envelope.fetched.url,
                score,
                suggestions: &envelope.suggestions,
            },
        )?;
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
            println!("{} {}", "Fetch".bold(), e.fetched.url.dimmed());
            println!(
                "  HTTP {}   {} bytes   score: {}/100",
                e.fetched.status,
                e.fetched.bytes,
                score_colour(e.score)
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
