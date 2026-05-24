//! Fetch a live URL, run it through the audit pipeline, and return the
//! same envelope as `audit` plus a `fetched` metadata block.
//!
//! Owns the network concerns — timeout, redirects, user-agent — so the
//! plain `audit` command stays pure-filesystem.

use serde::Serialize;
use std::io::Write;
use std::time::Duration;

use crate::audit::{self, AuditError};
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
}

pub fn run(ctx: Ctx, url: String, fail_under: Option<u32>) -> Result<(), AppError> {
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

    let body = response
        .into_string()
        .map_err(|e| AppError::Transient(format!("reading body: {e}")))?;
    let bytes = body.len();

    // Pick an extension the audit module recognises so it dispatches to
    // the right parser. Default to .html for anything that smells like
    // HTML or has no useful content-type.
    let ext = match content_type.as_deref() {
        Some(ct) if ct.contains("markdown") => "md",
        _ => "html",
    };

    let mut tmp = tempfile_in_state_dir(ext)?;
    tmp.as_file_mut().write_all(body.as_bytes())?;
    let path = tmp.path().to_path_buf();

    let report = audit::audit_file(&path).map_err(|e| match e {
        AuditError::NotFound(p) => AppError::InvalidInput(format!("file not found: {p}")),
        AuditError::UnsupportedType(t) => AppError::InvalidInput(format!(
            "unsupported file type: .{t}"
        )),
        AuditError::Io(e) => AppError::Io(e),
    })?;

    let score = report.score;
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
        },
        keywords: report.keywords,
        entities: report.entities,
        evidence: report.evidence,
        voice: report.voice,
        position_bias: report.position_bias,
        freshness: report.freshness,
        suggestions: report.suggestions,
    };

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

    if let Some(threshold) = fail_under
        && score < threshold
    {
        return Err(AppError::QualityGate { score, threshold });
    }

    Ok(())
}

fn tempfile_in_state_dir(ext: &str) -> Result<tempfile::NamedTempFile, AppError> {
    let dir = std::env::temp_dir();
    let f = tempfile::Builder::new()
        .prefix("aiseo-fetch-")
        .suffix(&format!(".{ext}"))
        .tempfile_in(dir)?;
    Ok(f)
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
