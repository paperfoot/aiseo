# aiseo

Agent-first CLI for **SEO**, **GEO** (generative engine optimisation), and **AEO** (answer engine optimisation) audits.

Built on [agent-cli-framework](https://github.com/199-biotechnologies/agent-cli-framework). The binary is the interface: every command emits a JSON envelope when piped, coloured human output in a terminal, and `aiseo agent-info` returns the full machine-readable capability manifest.

This CLI is designed for coding agents (Claude Code, Codex CLI, Gemini CLI) to use as a tool. Humans get a colour fallback, but the surface is shaped around what an agent needs to make decisions about a page.

## Install

```bash
cargo install --path .
```

Then drop the wrapper skill into your AI agent platforms:

```bash
aiseo skill install
```

This writes a small `SKILL.md` into `~/.claude/skills/aiseo/`, `~/.codex/skills/aiseo/`, and `~/.gemini/skills/aiseo/`. The skill itself is a one-screen pointer — the documentation lives inside the binary via `aiseo agent-info`.

## Use

Four user commands:

```bash
aiseo audit page.html                       # full audit of a local file
aiseo audit page.html --fail-under 80       # CI gate: exit 1 if score < 80
aiseo audit page.html --out audit.sarif     # GitHub Code Scanning
aiseo audit page.html --out audit.md        # committable Markdown report
aiseo audit page.html --factors meta,og     # only show meta+OG suggestions

aiseo fetch https://example.com             # audit a live URL
aiseo fetch https://example.com --fail-under 75 --out audit.sarif

aiseo schema faq --qa 'What is GEO?::Generative Engine Optimisation.'
aiseo schema article --title '...' --description '...' --date-published 2026-05-24 --author 'Dr Jane Smith' --credentials MD
aiseo schema howto --name 'Lower LDL' --step 'See your GP' --step 'Start a statin'
aiseo schema organization --name '199 Bio' --url https://199.bio
aiseo schema person --name 'Boris Djordjevic' --job-title 'Founder'

aiseo agent-info                            # full machine-readable manifest
aiseo skill install                         # drop a tiny SKILL.md into ~/.claude/skills/aiseo/
```

### Compose with anything that emits HTML or Markdown

`aiseo audit -` reads from stdin. HTML vs Markdown is sniffed from the first non-whitespace character. Lets the CLI plug into any extraction tool without growing its own crawler:

```bash
# Plain curl for static pages
curl -s https://example.com | aiseo audit -

# Via the `search` CLI for JS-heavy or anti-bot pages (firecrawl, stealth, browserless)
search search -q https://js-heavy.com -m scrape --json | jq -r '.results[0].snippet' | aiseo audit -

# Pre-deploy: pipe a built page through aiseo as part of the build
cat dist/blog/post.html | aiseo audit - --fail-under 80
```

Markup-only signals (meta tags, OG, schema, freshness) require raw HTML — most extraction tools strip these, so the markup half of the audit will come back empty. Content signals (keywords, entities, voice, position-bias) work on either.

### What `audit` returns

A single typed JSON envelope. Agents read sub-objects directly; humans read the suggestion list.

- `meta` — `<title>`, description, keywords, author, canonical
- `open_graph`, `twitter_card` — social preview surfaces
- `schema_types` — every `@type` found in JSON-LD blocks (e.g. `["Article", "FAQPage"]`)
- `content` — H1/H2/H3 lists, word count, presence flags
- **`keywords`** — `{ primary[], questions[], density{} }`
- **`entities`** — `{ people[{name, credentials?}], organizations[] }`
- **`evidence`** — `{ stat_count, quote_count, unsupported_claims[] }`
- **`voice`** — `{ featured_snippet_candidate, speakable_eligible, avg_sentence_words }`
- `position_bias` — word-offset percentages of TL;DR / first stat / first credential, with warnings when high-leverage signals sit past the first 30% of the page
- `freshness` — `dateModified`, `datePublished`, days since last modification, year mentions
- `score` — rough 0–100, weighted toward AI-citation surface
- **`score_breakdown`** — per-component deductions: `{ name, deducted, reason }[]`
- `suggestions` — flat list of concrete next actions

### Output formats

| `--out` extension | What you get | Use case |
|---|---|---|
| `.json` | Pretty JSON envelope | Programmatic, default |
| `.md` | Markdown report | Commit into a repo |
| `.sarif` | SARIF 2.1.0 | GitHub Code Scanning annotations |

## Why this exists

The Python skill at [`claude-skill-seo-geo-optimizer`](https://github.com/199-biotechnologies/claude-skill-seo-geo-optimizer) was a 13-script bundle stitched together with regex and `urllib`. It worked, but the install footprint was big, the parsing was brittle, and it was ergonomically wrong for an agent: too many subcommands, too much documentation in the skill file, and no way to call it without Python on the box.

`aiseo` collapses the same surface into a single binary, parses HTML with `scraper` instead of regex, returns honest typed JSON, and ships zero documentation in the skill — the binary describes itself.

## Research basis (May 2026)

The audit's heuristics are grounded in the post-Gemini-3 AI search landscape:

- **Position bias** — first ~30% of a page captures ~44% of AI-search citations (iPullRank, AIBoost 2026).
- **Schema** — useful for entity clarity and rich results, but only ~+2.4% AI Mode citation lift in the Ahrefs 1,885-page test. Don't oversell it.
- **Freshness** — Perplexity and the post-Gemini-3 AI Overviews both lift recently-modified content.
- **Named credentials and primary-source citations** — held up in the AgenticGEO and Citation Selection vs Absorption studies (2026).
- **FAQ rich results** — retired by Google on 7 May 2026, but FAQ schema still matters for Bing, Brave, DuckDuckGo, and as a structural signal for AI platforms.

## Exit codes

- `0` — success
- `1` — transient (IO / network) — retry
- `2` — config error — fix setup
- `3` — bad input — fix arguments
- `4` — rate limited — wait and retry

## Licence

MIT. © Boris Djordjevic, 199 Biotechnologies.
