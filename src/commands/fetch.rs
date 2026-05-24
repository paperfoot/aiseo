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
    performance: audit::Performance,
    link_graph: audit::LinkGraph,
    suggestions: Vec<String>,
}

#[derive(Serialize)]
struct FetchInfo {
    url: String,
    status: u16,
    content_type: Option<String>,
    bytes: usize,
    fetched_at: String,
    /// `X-Robots-Tag` response header — header-level indexing directive.
    /// Mirrors `<meta name="robots">` but only visible to the fetcher,
    /// not to a file-system audit. Present-and-restrictive (`noindex`,
    /// `nofollow`, `noai`, `noimageai`, `none`) is the finding.
    x_robots_tag: Option<String>,
    /// `Content-Encoding` — `br`, `gzip`, `zstd`, or absent. Absent on
    /// a non-trivial HTML response is a real performance miss.
    content_encoding: Option<String>,
    /// `Cache-Control` value, if any. Surfaces public/private + max-age.
    cache_control: Option<String>,
    /// `Last-Modified` header.
    last_modified: Option<String>,
    /// `Server` header banner, if disclosed.
    server: Option<String>,
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
        // 4xx is client error — retry won't help. 5xx and network errors
        // are transient. The old behaviour mapped both to Transient,
        // which made CI retry loops chase 404s forever.
        ureq::Error::Status(code, _) if (400..500).contains(&code) => {
            AppError::InvalidInput(format!("HTTP {code} fetching {url}"))
        }
        ureq::Error::Status(code, _) => AppError::Transient(format!(
            "HTTP {code} fetching {url}"
        )),
        ureq::Error::Transport(t) => AppError::Transient(format!("network error: {t}")),
    })?;

    let status = response.status();
    let content_type = response.header("content-type").map(str::to_string);
    let x_robots_tag = response.header("x-robots-tag").map(str::to_string);
    let content_encoding = response.header("content-encoding").map(str::to_string);
    let cache_control = response.header("cache-control").map(str::to_string);
    let last_modified = response.header("last-modified").map(str::to_string);
    let server = response.header("server").map(str::to_string);

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
    // Header-level indexing directive: X-Robots-Tag noindex/nofollow/
    // noai/noimageai/none. Add as a suggestion so the user sees it next
    // to the in-body robots-meta finding.
    let mut extra_suggestions: Vec<String> = Vec::new();
    if let Some(xrt) = x_robots_tag.as_deref() {
        let l = xrt.to_ascii_lowercase();
        let has = |needle: &str| l.split([',', ';']).any(|t| t.trim() == needle);
        if has("noindex") || has("none") {
            extra_suggestions.push(format!(
                "X-Robots-Tag header is `{xrt}`. Page excluded from index at the header level — meta-robots can't override this."
            ));
        }
        if has("nofollow") && !has("noindex") {
            extra_suggestions.push(format!(
                "X-Robots-Tag header includes `nofollow`. Links on this page pass no authority."
            ));
        }
        if has("noimageindex") {
            extra_suggestions.push(
                "X-Robots-Tag includes `noimageindex`. Page images won't appear in image search.".into(),
            );
        }
        // Codex 2026-05-24: `noai`/`noimageai` are non-standard. Google
        // documents `Google-Extended` via robots.txt as the supported
        // AI-training control; Anthropic documents ClaudeBot/Claude-User
        // user-agent controls in robots.txt. Surface the directive as
        // observed, do not claim major crawlers honour it.
        if has("noai") || has("noimageai") {
            extra_suggestions.push(format!(
                "X-Robots-Tag includes `noai`/`noimageai` (non-standard — Google and Anthropic document robots.txt user-agent controls instead, not these directives). Treat as a signal of intent, not enforcement."
            ));
        }
    }
    if content_encoding.as_deref().is_none() && bytes > 50_000 {
        extra_suggestions.push(format!(
            "{} KB response with no Content-Encoding (no Brotli/gzip). Compression cuts transfer size 70–85%.",
            bytes / 1024,
        ));
    }

    if !extra_suggestions.is_empty() {
        report.suggestions.splice(0..0, extra_suggestions);
    }

    let envelope = FetchEnvelope {
        fetched: FetchInfo {
            url: url.clone(),
            status,
            content_type,
            bytes,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            x_robots_tag,
            content_encoding,
            cache_control,
            last_modified,
            server,
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
        performance: report.performance,
        link_graph: report.link_graph,
        suggestions: report.suggestions,
    };

    if let Some(out_raw) = out.as_ref() {
        let out_path = report::resolve_out_path(out_raw);
        let format = ReportFormat::from_extension(&out_path)?;
        report::write(
            format,
            &out_path,
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
