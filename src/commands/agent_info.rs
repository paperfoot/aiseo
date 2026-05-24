/// Machine-readable capability manifest. Always JSON — agents bootstrap
/// the tool's surface from this one call.
pub fn run() {
    let name = env!("CARGO_PKG_NAME");
    let config_path = crate::config::config_path();

    let info = serde_json::json!({
        "name": name,
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "designed_for": "coding agents (Claude Code, Codex CLI, Gemini CLI). Humans get a colour fallback.",
        "commands": {
            "audit": {
                "description": "Full audit of an HTML or Markdown file. Returns metadata, schema types, content structure, position-bias signals, freshness, score breakdown, and a flat suggestion list.",
                "args": [
                    {
                        "name": "file",
                        "kind": "positional",
                        "type": "path",
                        "required": true,
                        "description": "Path to .html, .htm, .md, or .mdx file"
                    }
                ],
                "options": [
                    {
                        "name": "--fail-under",
                        "type": "int",
                        "required": false,
                        "description": "Exit 1 if the audit score is below this threshold. Useful in CI."
                    }
                ],
                "output_shape": {
                    "file": "string",
                    "file_type": "html | markdown",
                    "score": "0..100 (rough; weighted toward AI-citation surface)",
                    "score_breakdown": "{ total, components: [{ name, deducted, reason }] }",
                    "meta": "{ title, description, keywords, author, canonical }",
                    "open_graph": "{ title, description, image, url, type }",
                    "twitter_card": "{ card, title, description, image }",
                    "schema_types": "[\"Article\", \"FAQPage\", ...]",
                    "content": "{ word_count, h1[], h2[], h3[], has_tldr, has_faq, has_author, has_credentials }",
                    "position_bias": "{ total_words, tldr_position_pct, first_stat_position_pct, first_credential_position_pct, warnings[] }",
                    "freshness": "{ date_modified, date_published, days_since_modified, year_mentions[], current_year }",
                    "suggestions": "[string, ...]"
                }
            },
            "agent-info": {
                "description": "This manifest",
                "aliases": ["info"],
                "args": [],
                "options": []
            },
            "skill install": {
                "description": "Install skill file to ~/.claude/skills/, ~/.codex/skills/, ~/.gemini/skills/",
                "args": [],
                "options": []
            },
            "skill status": {
                "description": "Report which agent platforms have the skill installed and current",
                "args": [],
                "options": []
            },
            "config show": {
                "description": "Display effective merged configuration",
                "args": [],
                "options": []
            },
            "config path": {
                "description": "Show configuration file path",
                "args": [],
                "options": []
            },
            "update": {
                "description": "Self-update binary from GitHub Releases",
                "args": [],
                "options": [
                    {
                        "name": "--check",
                        "type": "bool",
                        "required": false,
                        "default": false,
                        "description": "Check only, don't install"
                    }
                ]
            }
        },
        "global_flags": {
            "--json": {
                "description": "Force JSON output (auto-enabled when piped)",
                "type": "bool",
                "default": false
            },
            "--quiet": {
                "description": "Suppress informational output",
                "type": "bool",
                "default": false
            }
        },
        "exit_codes": {
            "0": "Success",
            "1": "Transient error (IO, network) — retry",
            "2": "Config error — fix setup",
            "3": "Bad input — fix arguments",
            "4": "Rate limited — wait and retry"
        },
        "envelope": {
            "version": "1",
            "success": "{ version, status, data }",
            "error": "{ version, status, error: { code, message, suggestion } }"
        },
        "config": {
            "path": config_path.display().to_string(),
            "env_prefix": format!("{}_", name.to_uppercase())
        },
        "auto_json_when_piped": true,
        "research_basis": "https://github.com/paperfoot/aiseo/blob/main/docs/research.md"
    });
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}
