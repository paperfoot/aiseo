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
                        "description": "Path to .html, .htm, .md, or .mdx file. Pass `-` to read from stdin (HTML vs Markdown sniffed from the first non-whitespace character)."
                    }
                ],
                "options": [
                    {
                        "name": "--fail-under",
                        "type": "int",
                        "required": false,
                        "description": "Exit 1 if the audit score is below this threshold. Useful in CI."
                    },
                    {
                        "name": "--out",
                        "type": "path",
                        "required": false,
                        "description": "Write report to file. Format auto-detected from extension: .json, .html (Iowan-Old-Style printable), .sarif (GitHub Code Scanning)."
                    },
                    {
                        "name": "--factors",
                        "type": "string",
                        "required": false,
                        "description": "Comma-separated factor allow-list. Drops suggestions and score_breakdown components from other categories. Valid: meta, og, content, schema, freshness, position."
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
                    "content": "{ word_count, h1[], h2[], h3[], has_tldr, has_faq, has_author, has_credentials, image_count, missing_alt_count, html_lang, headings_in_order: [{level, text}], hreflangs[], noscript_kind: 'absent'|'boilerplate_only'|'substantive' }",
                    "keywords": "{ primary: [{ term, count }], questions: [string], density: { term: pct } }",
                    "entities": "{ people: [{ name, credentials? }], organizations: [string] }",
                    "evidence": "{ stat_count, quote_count, unsupported_claims: [{ snippet, position_pct }] }",
                    "voice": "{ featured_snippet_candidate: string|null, speakable_eligible: bool, avg_sentence_words }",
                    "position_bias": "{ total_words, tldr_position_pct, first_stat_position_pct, first_credential_position_pct, warnings[] }",
                    "freshness": "{ date_modified, date_published, days_since_modified, year_mentions[], current_year }",
                    "ai_slop": "{ signals: [{ kind, confidence, snippet, position_pct }], density_per_1000_words, verdict: 'clean' | 'suspicious' | 'likely_ai' }",
                    "information_gain": "{ score: 0..10, counts: { named_quotes, sample_sizes, yoy_deltas, first_person_evidence, method_disclosure, numbered_citations }, samples[] }",
                    "metatext": "{ signals: [{ kind, confidence, snippet, position_pct }], heading_skeleton: { jaccard, matched[] }, weighted_score_per_1000_words, verdict: 'clean' | 'suspicious' | 'metatext_heavy' }",
                    "copy_precision": "{ score: 0..10, counts, densities, verdict: 'tight' | 'mid' | 'padded' }",
                    "design_slop": "{ findings: [{ id, snippet }], counts: { rule_id: count }, verdict: 'clean' | 'minor' | 'suspicious' | 'heavy' }",
                    "suggestions": "[string, ...]"
                }
            },
            "verify": {
                "description": "Re-audit a file and diff against a previous audit JSON. Exit 1 if any previous suggestion is still present or a new one regressed. Use in an agent loop to stop the agent from claiming work it did not finish.",
                "args": [
                    {
                        "name": "before",
                        "kind": "positional",
                        "type": "path",
                        "required": true,
                        "description": "Path to a previous `aiseo audit ... --out before.json`. Accepts envelope or raw audit JSON."
                    },
                    {
                        "name": "current",
                        "kind": "positional",
                        "type": "path",
                        "required": true,
                        "description": "Current file to re-audit. Pass `-` for stdin."
                    }
                ],
                "options": [],
                "output_shape": {
                    "previous": "{ file, score }",
                    "current":  "{ file, score }",
                    "delta":    "{ score_change, fixed[], regressed[], still_present[] }",
                    "verdict":  "pass | fail"
                },
                "exit_codes": "0 if pass (no regressions, no still-present items); 1 if fail (verify_failed)"
            },
            "fetch": {
                "description": "Fetch a live URL and audit the response body. Same envelope as `audit` plus a `fetched` metadata block. Use this for deployed pages and competitor audits.",
                "args": [
                    {
                        "name": "url",
                        "kind": "positional",
                        "type": "string",
                        "required": true,
                        "description": "HTTP/HTTPS URL"
                    }
                ],
                "options": [
                    {
                        "name": "--fail-under",
                        "type": "int",
                        "required": false,
                        "description": "Exit 1 if score below threshold"
                    },
                    {
                        "name": "--out",
                        "type": "path",
                        "required": false,
                        "description": "Write report to file. Format auto-detected from extension: .json, .html, .sarif."
                    },
                    {
                        "name": "--factors",
                        "type": "string",
                        "required": false,
                        "description": "Comma-separated factor allow-list. Same vocabulary as audit."
                    }
                ],
                "output_shape": {
                    "fetched": "{ url, status, content_type, bytes, fetched_at }",
                    "...": "all `audit` fields (file, score, meta, content, keywords, entities, evidence, voice, position_bias, freshness, suggestions)"
                }
            },
            "schema": {
                "description": "Generate a JSON-LD block for a given schema.org type. Output is ready to paste into a <script type=\"application/ld+json\"> block.",
                "subcommands": {
                    "faq":          "--qa 'QUESTION::ANSWER' (repeatable, ≥1)",
                    "article":      "--title --description --date-published --author [--credentials --author-url --org-name --org-url --url --image --date-modified]",
                    "howto":        "--name --step ... [--description]",
                    "organization": "--name [--url --logo --same-as URL]",
                    "person":       "--name [--job-title --credentials --url --works-for]"
                },
                "output_shape": {
                    "schema_type": "FAQPage | Article | HowTo | Organization | Person",
                    "json_ld":     "valid JSON-LD object with @context and @type"
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
        "research_basis": "https://github.com/paperfoot/aiseo"
    });
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}
