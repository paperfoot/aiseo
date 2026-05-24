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
        /// Path to .html, .htm, .md, or .mdx file
        file: PathBuf,
        /// Exit 1 if the audit score is below this threshold. Useful in CI.
        #[arg(long, value_name = "SCORE")]
        fail_under: Option<u32>,
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
  aiseo audit page.html | jq '.score_breakdown'  # see deductions
  aiseo audit page.html --quiet --json > audit.json";

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
