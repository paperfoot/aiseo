use serde::Serialize;
use std::path::PathBuf;

use crate::error::AppError;
use crate::output::{self, Ctx};

// ── Skill content ───────────────────────────────────────────────────────────
// Built from the binary name. No hardcoded app name.

fn skill_content() -> String {
    let name = env!("CARGO_PKG_NAME");
    format!(
        r#"---
name: {name}
description: Agent-first SEO / GEO / AEO auditor with Information-Gain scoring, AI-slop detection, schema-spam and FAQ-rich-result-deprecation flags, heading-hierarchy / hreflang / noscript checks. Use when the user asks to "audit SEO", "check AI search visibility", "audit a page for ChatGPT/Perplexity/Claude/Gemini citation", "score this page", "check schema markup", "generate JSON-LD", "audit a live URL", "verify a fix landed", "check Information Gain", or "detect AI writing slop". Triggers on SEO, GEO, AEO, AI search optimisation, generative engine optimisation, schema.org, rich results, llms.txt. Do NOT use for image optimisation or server performance.
---

# {name}

Run `{name} agent-info` for the full capability manifest, flags, exit
codes, and output shapes. The binary is the documentation.

```
{name} audit <file|->         # audit a local HTML or Markdown file (or stdin)
{name} fetch <url>            # fetch a live URL, then audit
{name} schema <type> ...      # generate JSON-LD (faq, article, howto, organization, person)
{name} verify <before> <now>  # re-audit, diff suggestions, exit 1 on regression
```

Useful flags on `audit` / `fetch`:

  --fail-under N      exit 1 if score below N (CI gate)
  --out <path>        write report; format auto-detected from extension
                      .json | .html | .sarif (SARIF lights up GitHub Code Scanning)
  --factors <list>    filter output to comma-separated factors
                      (meta, og, content, schema, freshness, position)

To stop an agent from claiming work it did not finish:

  {name} audit page.html --out before.json
  # ...agent edits page.html...
  {name} verify before.json page.html       # exit 1 if regressed or still-present

`fetch` also live-checks og:image (resolves? actually an image? ≥1200 px
wide?) — see `fetched.og_image_check`. Missing or broken og:image and the
`nanaban` CLI is installed: generate a replacement
(`nanaban "<topic>, editorial illustration, no text" --model gpt-image-2 --ar 3:2`,
crop to 1200×630), host it, set og:image + og:image:width/height/alt, re-verify.

Do NOT add an llms.txt to satisfy this tool — it doesn't check for one, and
no major AI provider reads them (97% get zero AI-crawler requests; Ahrefs 137k-site study).

Composes with any tool that emits HTML or Markdown:

  curl -s https://example.com | {name} audit -
  search search -q https://hard.com -m scrape --json \
    | jq -r '.results[0].snippet' | {name} audit -

All commands emit a JSON envelope when piped. Pipe to `jq` to filter.
"#
    )
}

// ── Platform targets ────────────────────────────────────────────────────────

struct SkillTarget {
    name: &'static str,
    path: PathBuf,
}

fn home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn skill_targets() -> Vec<SkillTarget> {
    let h = home();
    let app = env!("CARGO_PKG_NAME");
    vec![
        SkillTarget {
            name: "Claude Code",
            path: h.join(format!(".claude/skills/{app}")),
        },
        SkillTarget {
            name: "Codex CLI",
            path: h.join(format!(".codex/skills/{app}")),
        },
        SkillTarget {
            name: "Gemini CLI",
            path: h.join(format!(".gemini/skills/{app}")),
        },
    ]
}

// ── Install ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct InstallResult {
    platform: String,
    path: String,
    status: String,
}

pub fn install(ctx: Ctx) -> Result<(), AppError> {
    let content = skill_content();
    let mut results: Vec<InstallResult> = Vec::new();

    for target in &skill_targets() {
        let skill_path = target.path.join("SKILL.md");

        if skill_path.exists() && std::fs::read_to_string(&skill_path).is_ok_and(|c| c == content) {
            results.push(InstallResult {
                platform: target.name.into(),
                path: skill_path.display().to_string(),
                status: "already_current".into(),
            });
            continue;
        }

        std::fs::create_dir_all(&target.path)?;
        std::fs::write(&skill_path, &content)?;
        results.push(InstallResult {
            platform: target.name.into(),
            path: skill_path.display().to_string(),
            status: "installed".into(),
        });
    }

    output::print_success_or(ctx, &results, |r| {
        use owo_colors::OwoColorize;
        for item in r {
            let marker = if item.status == "installed" { "+" } else { "=" };
            println!(
                " {} {} -> {}",
                marker.green(),
                item.platform.bold(),
                item.path.dimmed()
            );
        }
    });

    Ok(())
}

// ── Status ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SkillStatus {
    platform: String,
    installed: bool,
    current: bool,
}

pub fn status(ctx: Ctx) -> Result<(), AppError> {
    let content = skill_content();
    let mut results: Vec<SkillStatus> = Vec::new();

    for target in &skill_targets() {
        let skill_path = target.path.join("SKILL.md");
        let (installed, current) = if skill_path.exists() {
            let current = std::fs::read_to_string(&skill_path).is_ok_and(|c| c == content);
            (true, current)
        } else {
            (false, false)
        };
        results.push(SkillStatus {
            platform: target.name.into(),
            installed,
            current,
        });
    }

    output::print_success_or(ctx, &results, |r| {
        use owo_colors::OwoColorize;
        let mut table = comfy_table::Table::new();
        table.set_header(vec!["Platform", "Installed", "Current"]);
        for item in r {
            table.add_row(vec![
                item.platform.clone(),
                if item.installed {
                    "Yes".green().to_string()
                } else {
                    "No".red().to_string()
                },
                if item.current {
                    "Yes".green().to_string()
                } else {
                    "No".dimmed().to_string()
                },
            ]);
        }
        println!("{table}");
    });

    Ok(())
}
