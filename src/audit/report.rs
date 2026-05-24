//! Multi-format report writers for the audit envelope.
//!
//! Format is dispatched by the file extension passed to `--out`:
//!   .json   pretty JSON envelope (same shape as stdout JSON)
//!   .html   self-contained single-page report in the print-brief style
//!   .sarif  SARIF 2.1.0, for GitHub Code Scanning
//!
//! Markdown output is intentionally absent. The HTML report opens in any
//! browser, prints cleanly, and reviews well in a PR diff; an extra .md
//! variant only adds rot.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Default folder where bare-filename `--out` values land.
/// `--out audit.html`            → `~/Documents/aiseo/audit.html`
/// `--out reports/audit.html`    → `./reports/audit.html` (honoured as-is)
/// `--out /tmp/audit.html`       → `/tmp/audit.html` (honoured as-is)
pub fn resolve_out_path(raw: &Path) -> PathBuf {
    if raw.is_absolute() || raw.components().count() > 1 {
        return raw.to_path_buf();
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push("Documents");
        p.push("aiseo");
        p.push(raw);
        p
    } else {
        raw.to_path_buf()
    }
}

#[derive(Clone, Copy)]
pub enum ReportFormat {
    Json,
    Html,
    Sarif,
}

impl ReportFormat {
    pub fn from_extension(path: &Path) -> Result<Self, AppError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "json" => Ok(Self::Json),
            "html" | "htm" => Ok(Self::Html),
            "sarif" => Ok(Self::Sarif),
            "" => Err(AppError::InvalidInput(
                "--out path needs a file extension (.json, .html, .sarif)".into(),
            )),
            other => Err(AppError::InvalidInput(format!(
                "--out: unsupported extension .{other} (expected .json, .html, .sarif)"
            ))),
        }
    }
}

/// What the report writers read from the audit. The envelope is the source
/// of truth — we re-serialise it to JSON and pull named sub-objects so the
/// HTML / SARIF writers stay compatible with both `audit` and `fetch`
/// envelopes (which share keys).
pub struct ReportInput<'a, E: Serialize> {
    pub envelope: &'a E,
    pub file_label: &'a str,
    pub score: u32,
    pub suggestions: &'a [String],
}

pub fn write<E: Serialize>(
    format: ReportFormat,
    out_path: &Path,
    input: &ReportInput<'_, E>,
) -> Result<(), AppError> {
    let body = match format {
        ReportFormat::Json => serde_json::to_string_pretty(&input.envelope)
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?,
        ReportFormat::Html => render_html(input)?,
        ReportFormat::Sarif => render_sarif(input)?,
    };
    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out_path, body)?;
    Ok(())
}

fn render_html<E: Serialize>(input: &ReportInput<'_, E>) -> Result<String, AppError> {
    let env: serde_json::Value = serde_json::to_value(input.envelope)
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;

    let score = input.score;
    let file = h(input.file_label);
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let version = env!("CARGO_PKG_VERSION");

    let score_pill = format!(
        r#"<span class="pill {}">{}/100</span>"#,
        score_band(score),
        score
    );

    let mut body = String::with_capacity(4096);

    body.push_str(&format!(
        r#"<header>
  <h1>{file}</h1>
  <div class="sub">aiseo audit .. {date} .. {score_pill}</div>
</header>
"#
    ));

    // ── Suggestions ──────────────────────────────────────────────────────────
    body.push_str("<h2>Findings</h2>\n");
    if input.suggestions.is_empty() {
        body.push_str("<p>No findings. Ship.</p>\n");
    } else {
        body.push_str("<ol>\n");
        for s in input.suggestions {
            body.push_str(&format!("  <li>{}</li>\n", h(s)));
        }
        body.push_str("</ol>\n");
    }

    // ── Score breakdown ──────────────────────────────────────────────────────
    if let Some(components) = env
        .get("score_breakdown")
        .and_then(|b| b.get("components"))
        .and_then(|c| c.as_array())
        && !components.is_empty()
    {
        body.push_str("<h2>Score</h2>\n<table>\n");
        body.push_str("  <tr><th>Component</th><th>Reason</th><th class=\"num\">Deducted</th></tr>\n");
        for c in components {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let reason = c.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            let ded = c.get("deducted").and_then(|v| v.as_u64()).unwrap_or(0);
            body.push_str(&format!(
                "  <tr><td>{}</td><td>{}</td><td class=\"num\">−{}</td></tr>\n",
                h(name),
                h(reason),
                ded
            ));
        }
        body.push_str("</table>\n");
    }

    // ── Found (metadata surface) ─────────────────────────────────────────────
    body.push_str("<h2>Found</h2>\n<table>\n");
    push_kv(&mut body, "Title", env.get("meta").and_then(|m| m.get("title")));
    push_kv(
        &mut body,
        "Description",
        env.get("meta").and_then(|m| m.get("description")),
    );
    push_kv_join(
        &mut body,
        "Schema",
        env.get("schema_types").and_then(|v| v.as_array()),
    );
    push_kv(
        &mut body,
        "og:image",
        env.get("open_graph").and_then(|o| o.get("image")),
    );
    push_kv(
        &mut body,
        "Modified",
        env.get("freshness").and_then(|f| f.get("date_modified")),
    );
    push_kv(
        &mut body,
        "Published",
        env.get("freshness").and_then(|f| f.get("date_published")),
    );
    body.push_str("</table>\n");

    // ── Signals (content shape + position-bias) ─────────────────────────────
    body.push_str("<h2>Signals</h2>\n<table>\n");
    if let Some(c) = env.get("content") {
        push_kv(&mut body, "Words", c.get("word_count"));
        push_kv_count(&mut body, "H1", c.get("h1"));
        push_kv_count(&mut body, "H2", c.get("h2"));
        push_kv_count(&mut body, "H3", c.get("h3"));
        push_kv_bool(&mut body, "TL;DR", c.get("has_tldr"));
        push_kv_bool(&mut body, "FAQ", c.get("has_faq"));
        push_kv_bool(&mut body, "Author", c.get("has_author"));
        push_kv_bool(&mut body, "Credentials", c.get("has_credentials"));
        push_kv(&mut body, "Images", c.get("image_count"));
        push_kv(&mut body, "Images w/o alt", c.get("missing_alt_count"));
        push_kv(&mut body, "html lang", c.get("html_lang"));
    }
    if let Some(p) = env.get("position_bias") {
        push_kv_pct(&mut body, "TL;DR position", p.get("tldr_position_pct"));
        push_kv_pct(&mut body, "First statistic", p.get("first_stat_position_pct"));
        push_kv_pct(&mut body, "First credential", p.get("first_credential_position_pct"));
    }
    body.push_str("</table>\n");

    // ── AI fingerprint (one-line verdict for each new module) ───────────────
    body.push_str("<h2>AI fingerprint</h2>\n<table>\n");
    push_kv(
        &mut body,
        "AI-slop (lexical)",
        env.get("ai_slop").and_then(|v| v.get("verdict")),
    );
    push_kv(
        &mut body,
        "Metatext",
        env.get("metatext").and_then(|v| v.get("verdict")),
    );
    push_kv(
        &mut body,
        "Design slop",
        env.get("design_slop").and_then(|v| v.get("verdict")),
    );
    push_kv(
        &mut body,
        "Copy precision",
        env.get("copy_precision").and_then(|v| v.get("verdict")),
    );
    push_kv(
        &mut body,
        "Information Gain",
        env.get("information_gain").and_then(|v| v.get("score")),
    );
    body.push_str("</table>\n");

    // ── AI-slop detail (lexical regex hits) ─────────────────────────────────
    if let Some(slop) = env.get("ai_slop") {
        let density = slop.get("density_per_1000_words").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if let Some(signals) = slop.get("signals").and_then(|v| v.as_array())
            && !signals.is_empty()
        {
            body.push_str(&format!(
                "<h2>AI-slop signals</h2>\n<p class=\"small\">density {:.1} per 1000 words</p>\n<table>\n",
                density
            ));
            body.push_str("  <tr><th>Kind</th><th>Snippet</th><th class=\"num\">at %</th></tr>\n");
            for s in signals.iter().take(15) {
                let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let snip = s.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                let pos = s.get("position_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                body.push_str(&format!(
                    "  <tr><td>{}</td><td>{}</td><td class=\"num\">{:.1}%</td></tr>\n",
                    h(kind), h(snip), pos
                ));
            }
            body.push_str("</table>\n");
        }
    }

    // ── Metatext detail (process narration / self-id / hedges) ──────────────
    if let Some(meta) = env.get("metatext") {
        let density = meta.get("weighted_score_per_1000_words").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let jaccard = meta.get("heading_skeleton").and_then(|s| s.get("jaccard")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let matched: Vec<String> = meta
            .get("heading_skeleton")
            .and_then(|s| s.get("matched"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();

        if let Some(signals) = meta.get("signals").and_then(|v| v.as_array())
            && (!signals.is_empty() || jaccard >= 0.2)
        {
            body.push_str(&format!(
                "<h2>Metatext</h2>\n<p class=\"small\">density {:.2} / 1000 words .. heading-skeleton Jaccard {:.2}{}</p>\n",
                density,
                jaccard,
                if matched.is_empty() {
                    String::new()
                } else {
                    format!(" .. matched: {}", matched.join(", "))
                }
            ));
            if !signals.is_empty() {
                body.push_str("<table>\n  <tr><th>Kind</th><th>Snippet</th><th class=\"num\">at %</th></tr>\n");
                for s in signals.iter().take(15) {
                    let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                    let snip = s.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                    let pos = s.get("position_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    body.push_str(&format!(
                        "  <tr><td>{}</td><td>{}</td><td class=\"num\">{:.1}%</td></tr>\n",
                        h(kind), h(snip), pos
                    ));
                }
                body.push_str("</table>\n");
            }
        }
    }

    // ── Design slop detail (CSS / class hits) ───────────────────────────────
    if let Some(design) = env.get("design_slop")
        && let Some(findings) = design.get("findings").and_then(|v| v.as_array())
        && !findings.is_empty()
    {
        body.push_str("<h2>Design slop</h2>\n<table>\n");
        body.push_str("  <tr><th>Rule</th><th>Snippet</th></tr>\n");
        for f in findings.iter().take(20) {
            let id = f.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let snip = f.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
            body.push_str(&format!("  <tr><td>{}</td><td>{}</td></tr>\n", h(id), h(snip)));
        }
        body.push_str("</table>\n");
    }

    // ── Copy precision detail (densities + counts) ──────────────────────────
    if let Some(cp) = env.get("copy_precision") {
        let score = cp.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        body.push_str(&format!(
            "<h2>Copy precision</h2>\n<p class=\"small\">{:.1} / 10</p>\n<table>\n",
            score
        ));
        if let Some(dens) = cp.get("densities").and_then(|v| v.as_object()) {
            let mut keys: Vec<&String> = dens.keys().collect();
            keys.sort();
            for k in keys {
                let v = dens.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                body.push_str(&format!(
                    "  <tr><td class=\"label\">{}</td><td>{:.2}</td></tr>\n",
                    h(k), v
                ));
            }
        }
        body.push_str("</table>\n");
    }

    // ── Information Gain detail (counts + verbatim samples) ─────────────────
    if let Some(ig) = env.get("information_gain") {
        let s = ig.get("score").and_then(|v| v.as_u64()).unwrap_or(0);
        body.push_str(&format!(
            "<h2>Information Gain</h2>\n<p class=\"small\">{} / 10 first-party signals</p>\n<table>\n",
            s
        ));
        if let Some(counts) = ig.get("counts").and_then(|v| v.as_object()) {
            let order = [
                "named_quotes",
                "sample_sizes",
                "yoy_deltas",
                "first_person_evidence",
                "method_disclosure",
                "numbered_citations",
            ];
            for k in order {
                if let Some(n) = counts.get(k).and_then(|v| v.as_u64()) {
                    body.push_str(&format!(
                        "  <tr><td class=\"label\">{}</td><td>{}</td></tr>\n",
                        h(k), n
                    ));
                }
            }
        }
        body.push_str("</table>\n");
        if let Some(samples) = ig.get("samples").and_then(|v| v.as_array())
            && !samples.is_empty()
        {
            body.push_str("<p class=\"small\">Samples:</p>\n<table>\n");
            body.push_str("  <tr><th>Kind</th><th>Snippet</th></tr>\n");
            for s in samples.iter().take(8) {
                let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let snip = s.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                body.push_str(&format!("  <tr><td>{}</td><td>{}</td></tr>\n", h(kind), h(snip)));
            }
            body.push_str("</table>\n");
        }
    }

    // ── Content depth (keywords / entities / voice) ─────────────────────────
    if let Some(kw) = env.get("keywords")
        && let Some(primary) = kw.get("primary").and_then(|v| v.as_array())
        && !primary.is_empty()
    {
        body.push_str("<h2>Keywords</h2>\n<table>\n  <tr><th>Term</th><th class=\"num\">Count</th></tr>\n");
        for k in primary.iter().take(8) {
            let t = k.get("term").and_then(|v| v.as_str()).unwrap_or("");
            let c = k.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            body.push_str(&format!("  <tr><td>{}</td><td class=\"num\">{}</td></tr>\n", h(t), c));
        }
        body.push_str("</table>\n");
        if let Some(qs) = kw.get("questions").and_then(|v| v.as_array())
            && !qs.is_empty()
        {
            body.push_str("<p class=\"small\">Questions detected:</p>\n<ul>\n");
            for q in qs.iter().take(6) {
                if let Some(s) = q.as_str() {
                    body.push_str(&format!("  <li>{}</li>\n", h(s)));
                }
            }
            body.push_str("</ul>\n");
        }
    }

    if let Some(ent) = env.get("entities") {
        let people = ent.get("people").and_then(|v| v.as_array());
        let orgs = ent.get("organizations").and_then(|v| v.as_array());
        let has_people = people.is_some_and(|p| !p.is_empty());
        let has_orgs = orgs.is_some_and(|o| !o.is_empty());
        if has_people || has_orgs {
            body.push_str("<h2>Entities</h2>\n<table>\n");
            if let Some(p) = people {
                for person in p.iter().take(8) {
                    let name = person.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let cred = person.get("credentials").and_then(|v| v.as_str()).unwrap_or("");
                    let display = if cred.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name}, {cred}")
                    };
                    body.push_str(&format!(
                        "  <tr><td class=\"label\">Person</td><td>{}</td></tr>\n",
                        h(&display)
                    ));
                }
            }
            if let Some(o) = orgs {
                for org in o.iter().take(10) {
                    if let Some(s) = org.as_str() {
                        body.push_str(&format!(
                            "  <tr><td class=\"label\">Organisation</td><td>{}</td></tr>\n",
                            h(s)
                        ));
                    }
                }
            }
            body.push_str("</table>\n");
        }
    }

    if let Some(voice) = env.get("voice") {
        let speakable = voice.get("speakable_eligible").and_then(|v| v.as_bool()).unwrap_or(false);
        let avg = voice.get("avg_sentence_words").and_then(|v| v.as_u64()).unwrap_or(0);
        let snippet = voice.get("featured_snippet_candidate").and_then(|v| v.as_str()).unwrap_or("");
        if avg > 0 || !snippet.is_empty() {
            body.push_str("<h2>Voice</h2>\n<table>\n");
            push_kv_bool(&mut body, "Speakable schema", Some(&serde_json::Value::Bool(speakable)));
            body.push_str(&format!(
                "  <tr><td class=\"label\">Avg sentence words</td><td>{}</td></tr>\n",
                avg
            ));
            body.push_str("</table>\n");
            if !snippet.is_empty() {
                body.push_str(&format!(
                    "<p class=\"small\">Featured-snippet candidate:</p>\n<blockquote style=\"border-left: 2px solid var(--rule); padding-left: 1em; color: var(--soft); font-style: italic;\">{}</blockquote>\n",
                    h(snippet)
                ));
            }
        }
    }

    // ── Evidence (stats + unsupported claims) ───────────────────────────────
    if let Some(ev) = env.get("evidence") {
        let stat_count = ev.get("stat_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let quote_count = ev.get("quote_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let claims = ev.get("unsupported_claims").and_then(|v| v.as_array());
        body.push_str("<h2>Evidence</h2>\n<table>\n");
        body.push_str(&format!(
            "  <tr><td class=\"label\">Statistics</td><td>{}</td></tr>\n",
            stat_count
        ));
        body.push_str(&format!(
            "  <tr><td class=\"label\">Quotations</td><td>{}</td></tr>\n",
            quote_count
        ));
        body.push_str("</table>\n");
        if let Some(c) = claims
            && !c.is_empty()
        {
            body.push_str("<p class=\"small\">Unsupported claims (no citation marker within 400 chars):</p>\n<table>\n");
            body.push_str("  <tr><th>Snippet</th><th class=\"num\">at %</th></tr>\n");
            for x in c.iter().take(10) {
                let snip = x.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                let pos = x.get("position_pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
                body.push_str(&format!(
                    "  <tr><td>{}</td><td class=\"num\">{:.1}%</td></tr>\n",
                    h(snip), pos
                ));
            }
            body.push_str("</table>\n");
        }
    }

    // ── Freshness (full block) ──────────────────────────────────────────────
    if let Some(f) = env.get("freshness") {
        body.push_str("<h2>Freshness</h2>\n<table>\n");
        push_kv(&mut body, "dateModified", f.get("date_modified"));
        push_kv(&mut body, "datePublished", f.get("date_published"));
        push_kv(&mut body, "Days since modified", f.get("days_since_modified"));
        push_kv(&mut body, "Current year", f.get("current_year"));
        if let Some(years) = f.get("year_mentions").and_then(|v| v.as_array())
            && !years.is_empty()
        {
            let years_str = years
                .iter()
                .filter_map(|y| y.as_u64().map(|n| n.to_string()))
                .collect::<Vec<_>>()
                .join(", ");
            body.push_str(&format!(
                "  <tr><td class=\"label\">Year mentions</td><td>{}</td></tr>\n",
                h(&years_str)
            ));
        }
        body.push_str("</table>\n");
    }

    // ── Headings in order (sanity table) ────────────────────────────────────
    if let Some(headings) = env
        .get("content")
        .and_then(|c| c.get("headings_in_order"))
        .and_then(|v| v.as_array())
        && !headings.is_empty()
    {
        body.push_str("<h2>Headings in order</h2>\n<table>\n  <tr><th>H</th><th>Text</th></tr>\n");
        for hd in headings.iter().take(30) {
            let level = hd.get("level").and_then(|v| v.as_u64()).unwrap_or(0);
            let text = hd.get("text").and_then(|v| v.as_str()).unwrap_or("");
            body.push_str(&format!(
                "  <tr><td class=\"num\">H{}</td><td>{}</td></tr>\n",
                level, h(text)
            ));
        }
        body.push_str("</table>\n");
    }

    // ── Footer ───────────────────────────────────────────────────────────────
    body.push_str(&format!(
        r#"<hr>
<p class="small">aiseo {version} .. <a href="https://github.com/paperfoot/aiseo">paperfoot/aiseo</a></p>
"#
    ));

    Ok(format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>aiseo .. {file}</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  :root {{
    --ink: #1d1a16;
    --soft: #6b665e;
    --rule: #d9d2c5;
    --page: #f6f1e7;
    --accent: #8a3a1f;
    --good: #355a3a;
  }}
  html, body {{ background: var(--page); color: var(--ink); }}
  body {{
    font: 16px/1.6 "Iowan Old Style", "Georgia", serif;
    max-width: 640px;
    margin: 6rem auto;
    padding: 0 1.5rem 6rem;
  }}
  header {{ margin-bottom: 3rem; }}
  h1 {{
    font-weight: 500;
    font-size: 1.6rem;
    letter-spacing: 0.01em;
    margin: 0 0 .35rem;
    word-break: break-all;
  }}
  .sub {{
    color: var(--soft);
    font-size: .92rem;
    font-style: italic;
  }}
  h2 {{
    font-weight: 500;
    font-size: 1.05rem;
    letter-spacing: .04em;
    text-transform: uppercase;
    color: var(--accent);
    margin: 2.6rem 0 .9rem;
  }}
  p {{ margin: .6rem 0; }}
  table {{
    width: 100%;
    border-collapse: collapse;
    margin: .4rem 0 1rem;
    font-size: .95rem;
  }}
  th, td {{
    text-align: left;
    padding: .55rem .4rem;
    border-bottom: 1px solid var(--rule);
    vertical-align: top;
  }}
  th {{
    font-weight: 500;
    color: var(--soft);
    font-size: .82rem;
    letter-spacing: .04em;
    text-transform: uppercase;
  }}
  td.num {{ text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }}
  td.label {{ color: var(--soft); width: 40%; }}
  ol {{ padding-left: 1.2rem; }}
  ol li {{ margin: .35rem 0; }}
  .small {{ font-size: .88rem; color: var(--soft); }}
  .pill {{
    display: inline-block;
    padding: .05rem .55rem;
    border: 1px solid currentColor;
    border-radius: 2px;
    font-size: .82rem;
    letter-spacing: .03em;
    font-style: normal;
  }}
  .pill.good {{ color: var(--good); }}
  .pill.mid {{ color: var(--accent); }}
  .pill.poor {{ color: var(--accent); border-width: 2px; }}
  hr {{
    border: none;
    border-top: 1px solid var(--rule);
    margin: 3rem 0 1rem;
  }}
  a {{ color: var(--accent); }}
</style>
</head>
<body>
{body}</body>
</html>
"#))
}

fn score_band(score: u32) -> &'static str {
    match score {
        85..=100 => "good",
        60..=84 => "mid",
        _ => "poor",
    }
}

fn push_kv(out: &mut String, label: &str, v: Option<&serde_json::Value>) {
    let val = v
        .and_then(|v| match v {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) if s.is_empty() => None,
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(if *b { "yes".into() } else { "no".into() }),
            _ => Some(v.to_string()),
        })
        .unwrap_or_else(|| "—".to_string());
    out.push_str(&format!(
        "  <tr><td class=\"label\">{}</td><td>{}</td></tr>\n",
        h(label),
        h(&val)
    ));
}

fn push_kv_join(out: &mut String, label: &str, v: Option<&Vec<serde_json::Value>>) {
    let val = v
        .map(|arr| {
            if arr.is_empty() {
                "—".to_string()
            } else {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        })
        .unwrap_or_else(|| "—".to_string());
    out.push_str(&format!(
        "  <tr><td class=\"label\">{}</td><td>{}</td></tr>\n",
        h(label),
        h(&val)
    ));
}

fn push_kv_count(out: &mut String, label: &str, v: Option<&serde_json::Value>) {
    let n = v.and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
    out.push_str(&format!(
        "  <tr><td class=\"label\">{}</td><td>{}</td></tr>\n",
        h(label),
        n
    ));
}

fn push_kv_bool(out: &mut String, label: &str, v: Option<&serde_json::Value>) {
    let b = v.and_then(|v| v.as_bool()).unwrap_or(false);
    let s = if b { "yes" } else { "no" };
    out.push_str(&format!(
        "  <tr><td class=\"label\">{}</td><td>{}</td></tr>\n",
        h(label),
        s
    ));
}

fn push_kv_pct(out: &mut String, label: &str, v: Option<&serde_json::Value>) {
    let s = match v.and_then(|v| v.as_f64()) {
        Some(p) => format!("{p:.1}%"),
        None => "—".to_string(),
    };
    out.push_str(&format!(
        "  <tr><td class=\"label\">{}</td><td>{}</td></tr>\n",
        h(label),
        h(&s)
    ));
}

fn h(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_sarif<E: Serialize>(input: &ReportInput<'_, E>) -> Result<String, AppError> {
    let results: Vec<_> = input
        .suggestions
        .iter()
        .map(|s| {
            serde_json::json!({
                "ruleId": "aiseo.audit.suggestion",
                "level": "warning",
                "message": { "text": s },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": input.file_label }
                    }
                }]
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "aiseo",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/paperfoot/aiseo",
                    "rules": [{
                        "id": "aiseo.audit.suggestion",
                        "name": "AuditSuggestion",
                        "shortDescription": { "text": "aiseo audit suggestion" },
                        "fullDescription": {
                            "text": "A heuristic suggestion from aiseo's audit. See the message for the specific recommendation."
                        },
                        "helpUri": "https://github.com/paperfoot/aiseo"
                    }]
                }
            },
            "results": results,
            "properties": {
                "aiseo.score": input.score
            }
        }]
    });

    serde_json::to_string_pretty(&sarif)
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}
