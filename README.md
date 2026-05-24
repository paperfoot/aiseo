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

```bash
aiseo audit page.html                  # full audit, coloured TTY output
aiseo audit page.html | jq             # JSON envelope when piped
aiseo audit post.md | jq '.suggestions'
aiseo audit page.html --json > audit.json
```

What `audit` returns:
- `meta` — `<title>`, description, keywords, author, canonical
- `open_graph`, `twitter_card` — social preview surfaces
- `schema_types` — every `@type` found in JSON-LD blocks (e.g. `["Article", "FAQPage"]`)
- `content` — H1/H2/H3 lists, word count, presence flags (TL;DR, FAQ, author, credentials)
- `position_bias` — word-offset percentage of TL;DR / first stat / first credential, with warnings when the high-leverage signals sit past the first 30% of the page
- `freshness` — `dateModified`, `datePublished`, days since last modification, year mentions
- `score` — rough 0–100, weighted toward AI-citation surface
- `suggestions` — flat list of concrete next actions

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
