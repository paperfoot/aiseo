//! Verify that a claimed fix actually landed.
//!
//! The orchestration gap: an LLM that says "I fixed it" is not evidence.
//! `aiseo verify <before.json> <current-file|->` re-runs the audit and
//! diffs the new suggestion list against the recorded one:
//!
//!   - `fixed[]`         suggestions present before, absent now
//!   - `regressed[]`     suggestions absent before, present now
//!   - `still_present[]` suggestions present in both
//!
//! Exit 0 only when the previous suggestions are all fixed AND nothing new
//! regressed. Exit 1 otherwise. Same semantics as `--fail-under` — the
//! audit data is on stdout, the gate flips the exit code.

use serde::Serialize;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

use crate::audit::{self, AuditError, ContentType};
use crate::error::AppError;
use crate::output::{self, Ctx};

#[derive(Serialize)]
struct VerifyEnvelope {
    previous: PreviousSnapshot,
    current: CurrentSnapshot,
    delta: Delta,
    verdict: &'static str,
}

#[derive(Serialize)]
struct PreviousSnapshot {
    file: String,
    score: u32,
}

#[derive(Serialize)]
struct CurrentSnapshot {
    file: String,
    score: u32,
}

#[derive(Serialize)]
struct Delta {
    score_change: i64,
    fixed: Vec<String>,
    regressed: Vec<String>,
    still_present: Vec<String>,
}

pub fn run(ctx: Ctx, before: PathBuf, current: PathBuf) -> Result<(), AppError> {
    // 1. Load the previous audit envelope.
    if !before.exists() {
        return Err(AppError::InvalidInput(format!(
            "previous audit not found: {}",
            before.display()
        )));
    }
    let prev_raw = std::fs::read_to_string(&before)?;
    let prev_env: serde_json::Value = serde_json::from_str(&prev_raw).map_err(|e| {
        AppError::InvalidInput(format!(
            "previous audit at {} is not valid JSON: {e}",
            before.display()
        ))
    })?;

    // Accept either a raw audit (top-level keys) or an envelope ({status, data}).
    let prev_data = if prev_env.get("status").is_some() && prev_env.get("data").is_some() {
        prev_env["data"].clone()
    } else {
        prev_env
    };

    let prev_file = prev_data
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>")
        .to_string();
    let prev_score = prev_data
        .get("score")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let prev_suggestions: BTreeSet<String> = prev_data
        .get("suggestions")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // 2. Re-audit the current file (or stdin).
    let report = if current.as_os_str() == "-" {
        let mut raw = String::new();
        std::io::stdin().read_to_string(&mut raw).map_err(AppError::Io)?;
        let ctype = ContentType::sniff(&raw);
        audit::audit_content(raw, ctype, "<stdin>".to_string())
    } else {
        audit::audit_file(&current)
    }
    .map_err(|e| match e {
        AuditError::NotFound(p) => AppError::InvalidInput(format!("file not found: {p}")),
        AuditError::UnsupportedType(t) => {
            AppError::InvalidInput(format!("unsupported file type: .{t}"))
        }
        AuditError::Io(e) => AppError::Io(e),
    })?;

    let curr_suggestions: BTreeSet<String> = report.suggestions.iter().cloned().collect();

    // 3. Diff. fixed = previous \ current. regressed = current \ previous.
    let fixed: Vec<String> = prev_suggestions
        .difference(&curr_suggestions)
        .cloned()
        .collect();
    let regressed: Vec<String> = curr_suggestions
        .difference(&prev_suggestions)
        .cloned()
        .collect();
    let still_present: Vec<String> = prev_suggestions
        .intersection(&curr_suggestions)
        .cloned()
        .collect();

    let score_change = report.score as i64 - prev_score as i64;
    let pass = still_present.is_empty() && regressed.is_empty();
    let verdict = if pass { "pass" } else { "fail" };

    let envelope = VerifyEnvelope {
        previous: PreviousSnapshot {
            file: prev_file,
            score: prev_score,
        },
        current: CurrentSnapshot {
            file: report.file.clone(),
            score: report.score,
        },
        delta: Delta {
            score_change,
            fixed,
            regressed,
            still_present,
        },
        verdict,
    };

    output::print_success_or(ctx, &envelope, |e| {
        use owo_colors::OwoColorize;
        let verdict_str = if pass {
            "pass".green().to_string()
        } else {
            "fail".red().to_string()
        };
        println!(
            "{} {} -> {} ({})  {}",
            "Verify".bold(),
            e.previous.score,
            e.current.score,
            score_sign(e.delta.score_change),
            verdict_str,
        );
        if !e.delta.fixed.is_empty() {
            println!("\n  {} ({})", "Fixed".green(), e.delta.fixed.len());
            for s in &e.delta.fixed {
                println!("   • {s}");
            }
        }
        if !e.delta.still_present.is_empty() {
            println!(
                "\n  {} ({})",
                "Still present".yellow(),
                e.delta.still_present.len()
            );
            for s in &e.delta.still_present {
                println!("   • {s}");
            }
        }
        if !e.delta.regressed.is_empty() {
            println!("\n  {} ({})", "Regressed".red(), e.delta.regressed.len());
            for s in &e.delta.regressed {
                println!("   • {s}");
            }
        }
    });

    if !pass {
        return Err(AppError::VerifyFailed {
            still_present: envelope.delta.still_present.len(),
            regressed: envelope.delta.regressed.len(),
        });
    }

    Ok(())
}

fn score_sign(d: i64) -> String {
    match d.signum() {
        1 => format!("+{d}"),
        -1 => d.to_string(),
        _ => "±0".to_string(),
    }
}
