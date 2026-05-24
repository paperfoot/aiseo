//! Multi-format report writers for the audit envelope.
//!
//! Format is dispatched by the file extension passed to `--out`:
//!   .json   — pretty JSON envelope (same shape as stdout JSON)
//!   .md     — Markdown, suitable for committing into a repo
//!   .sarif  — SARIF 2.1.0, suitable for GitHub Code Scanning
//!
//! SARIF is the agent-leverage format: dropping `aiseo audit --out
//! audit.sarif page.html` into a GitHub Action lights up Code Scanning
//! annotations on the PR for free.

use serde::Serialize;
use std::path::Path;

use crate::error::AppError;

#[derive(Clone, Copy)]
pub enum ReportFormat {
    Json,
    Markdown,
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
            "md" | "markdown" => Ok(Self::Markdown),
            "sarif" => Ok(Self::Sarif),
            "" => Err(AppError::InvalidInput(
                "--out path needs a file extension (.json, .md, .sarif)".into(),
            )),
            other => Err(AppError::InvalidInput(format!(
                "--out: unsupported extension .{other} (expected .json, .md, .sarif)"
            ))),
        }
    }
}

/// What every report writer needs from the audit. Kept narrow on purpose
/// so commands::audit and commands::fetch can both supply it.
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
        ReportFormat::Markdown => render_markdown(input),
        ReportFormat::Sarif => render_sarif(input)?,
    };
    std::fs::write(out_path, body)?;
    Ok(())
}

fn render_markdown<E: Serialize>(input: &ReportInput<'_, E>) -> String {
    let mut out = String::new();
    out.push_str(&format!("# aiseo audit — {}\n\n", input.file_label));
    out.push_str(&format!("**Score:** {}/100\n\n", input.score));
    if input.suggestions.is_empty() {
        out.push_str("No suggestions. Ship it.\n");
    } else {
        out.push_str("## Suggestions\n\n");
        for s in input.suggestions {
            out.push_str(&format!("- {s}\n"));
        }
    }
    out.push('\n');
    out
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
