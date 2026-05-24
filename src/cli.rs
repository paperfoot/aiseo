use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub use crate::commands::schema::SchemaType;

const LONG_ABOUT: &str = "\
Agent-first CLI for SEO, GEO (generative engine optimisation), and AEO
(answer engine optimisation) audits.

The binary is the interface: every command emits structured JSON when piped,
human-readable output when run in a terminal. Run `aiseo agent-info` for the
full machine-readable capability manifest.";

#[derive(Parser)]
#[command(version, about = "Agent-first SEO / GEO / AEO auditor", long_about = LONG_ABOUT)]
pub struct Cli {
    /// Force JSON output even in a terminal
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress informational output
    #[arg(long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Full audit of an HTML or Markdown file: metadata, schema, content,
    /// position bias, freshness, and a flat suggestion list.
    #[command(after_long_help = AUDIT_HELP)]
    Audit {
        /// Path to .html, .htm, .md, or .mdx file, or `-` to read from stdin
        /// (auto-detects HTML vs Markdown by sniffing the first non-whitespace
        /// character).
        file: PathBuf,
        /// Exit 1 if the audit score is below this threshold. Useful in CI.
        #[arg(long, value_name = "SCORE")]
        fail_under: Option<u32>,
        /// Write the report to a file. Format is auto-detected from the
        /// extension: .json, .md, .sarif. SARIF lights up GitHub Code
        /// Scanning annotations.
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// Comma-separated list of factors to keep in the output:
        /// meta, og, content, schema, freshness, position. Omit to keep all.
        #[arg(long, value_name = "LIST")]
        factors: Option<String>,
    },
    /// Verify that a claimed fix actually landed. Loads a previous audit
    /// JSON, re-audits the current file, and diffs the suggestion lists.
    /// Exit 1 if any previous suggestion is still present or a new one
    /// regressed.
    #[command(after_long_help = VERIFY_HELP)]
    Verify {
        /// Path to a previous `aiseo audit ... --out before.json` (envelope
        /// or raw audit JSON both accepted).
        before: PathBuf,
        /// Current file to re-audit. Pass `-` for stdin.
        current: PathBuf,
    },
    /// Fetch a live URL and audit the response body. Same envelope as
    /// `audit` plus a `fetched: { url, status, content_type, bytes }` block.
    #[command(after_long_help = FETCH_HELP)]
    Fetch {
        /// HTTP/HTTPS URL to fetch and audit.
        url: String,
        /// Exit 1 if the audit score is below this threshold.
        #[arg(long, value_name = "SCORE")]
        fail_under: Option<u32>,
        /// Write the report to a file. Auto-detects format from extension
        /// (.json, .md, .sarif).
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
        /// Comma-separated list of factors to keep in the output.
        #[arg(long, value_name = "LIST")]
        factors: Option<String>,
    },
    /// Generate a JSON-LD block for a given schema.org type. Output is
    /// ready to paste into a `<script type="application/ld+json">` block.
    #[command(after_long_help = SCHEMA_HELP)]
    Schema {
        #[command(subcommand)]
        kind: SchemaType,
    },
    /// Machine-readable capability manifest
    #[command(visible_alias = "info")]
    AgentInfo,
    /// Manage skill file installation
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Self-update from GitHub Releases
    Update {
        /// Check only, don't install
        #[arg(long)]
        check: bool,
    },
    /// Hidden: deterministic exit-code trigger for contract tests
    #[command(hide = true)]
    Contract {
        /// Exit code to trigger (0-4)
        code: i32,
    },
}

const VERIFY_HELP: &str = "\
TIPS:
  • Run audit first with --out before.json, apply your fix, then verify
  • Exit 1 means: previous suggestions still present, or new ones regressed
  • Use in an agent loop to stop the agent from claiming work it didn't finish

EXAMPLES:
  aiseo audit page.html --out before.json
  # ...agent edits page.html...
  aiseo verify before.json page.html

  # Verify a deployed page against an earlier snapshot
  curl -s https://example.com | aiseo verify before.json -";

const FETCH_HELP: &str = "\
TIPS:
  • For local files, use `audit` instead — no network, no rate limit
  • Network errors are exit code 1 (transient) — retry
  • Use `--fail-under` for CI gates that test live deployments

EXAMPLES:
  aiseo fetch https://example.com/about
  aiseo fetch https://example.com --fail-under 75
  aiseo fetch https://example.com | jq '.score_breakdown.components'";

const SCHEMA_HELP: &str = "\
EXAMPLES:
  aiseo schema faq --qa 'What is GEO?::Generative Engine Optimisation.' --qa 'Why?::Citations.'
  aiseo schema article --title 'Optimal LDL in 2026' --description '...' --date-published 2026-05-24 --author 'Dr Jane Smith' --credentials MD
  aiseo schema howto --name 'Lower LDL' --step 'See your GP' --step 'Start a statin' --step 'Re-test in 6 weeks'
  aiseo schema organization --name '199 Biotechnologies' --url https://199.bio --logo https://199.bio/logo.png
  aiseo schema person --name 'Boris Djordjevic' --job-title 'Founder' --credentials MSc --url https://x.com/longevityboris";

const AUDIT_HELP: &str = "\
TIPS:
  • Pipe to `jq` for filtering: `aiseo audit page.html | jq '.suggestions'`
  • Score is rough and weighted toward AI-citation surfaces, not legacy SEO
  • Position bias warnings trigger when TL;DR / first stat sit past 30% of body
  • Use --fail-under in CI: `aiseo audit page.html --fail-under 80` exits 1 if score < 80
  • Read `score_breakdown.components[]` to know *which* deduction to fix next

EXAMPLES:
  aiseo audit ~/site/about.html
  aiseo audit ~/site/post.md | jq '{score, suggestions}'
  aiseo audit page.html --fail-under 80          # CI gate
  aiseo audit page.html --out audit.sarif        # GitHub Code Scanning
  aiseo audit page.html --out audit.md           # committable report
  aiseo audit page.html | jq '.score_breakdown'  # see deductions

  # Compose with anything that emits HTML or Markdown on stdout:
  curl -s https://example.com | aiseo audit -
  search search -q https://js-heavy.com -m scrape --json \\
    | jq -r '.results[0].snippet' | aiseo audit -";

#[derive(Subcommand)]
pub enum SkillAction {
    /// Write skill file to all detected agent platforms
    Install,
    /// Check which platforms have the skill installed
    Status,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Display effective merged configuration
    Show,
    /// Print configuration file path
    Path,
}
