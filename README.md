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
aiseo audit page.html --out audit.html      # printable, reviewable report
aiseo audit page.html --out audit.sarif     # GitHub Code Scanning annotations
aiseo audit page.html --factors meta,og     # only show meta + OG suggestions

aiseo fetch https://example.com             # audit a live URL
aiseo fetch https://example.com --fail-under 75 --out audit.sarif

aiseo verify before.json page.html          # re-audit + diff; exit 1 if regression
                                            # or any previous suggestion still present

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
- `open_graph`, `twitter_card` — preview surfaces for social *and* AI citation cards. Checked for done-well, not just present: absolute https `og:image`, declared `og:image:width/height` (without them the first share of a page renders imageless), `og:image:alt`, `og:description`, `og:url`↔canonical agreement, `og:type=article` on Article pages, and `twitter:card=summary_large_image` (the one `twitter:*` tag OG can't replace — X falls back per-field but renders the small card without it). `fetch` goes further and **probes the og:image URL live**: HTTP status, real content-type, and pixel dimensions sniffed from the file header (PNG/JPEG/GIF/WebP) — catching 404'd images, bot-challenge pages served as images, SVGs (most platforms refuse them), undersized and portrait images. Result lands in `fetched.og_image_check`.
- `schema_types` — every `@type` found in JSON-LD blocks (e.g. `["Article", "FAQPage"]`)
- `content` — H1/H2/H3 lists, word count, presence flags
- **`keywords`** — `{ primary[], questions[], density{} }`
- **`entities`** — `{ people[{name, credentials?}], organizations[] }`
- **`evidence`** — `{ stat_count, quote_count, unsupported_claims[] }`
- **`voice`** — `{ featured_snippet_candidate, speakable_eligible, avg_sentence_words }`
- **`ai_slop`** — `{ signals[], density_per_1000_words, verdict }`. Regex-only LLM-writing detector (negation pivots, `delve` family, `tapestry` cluster, false-conclusion openers, bold-colon headers, etc.). Em-dashes deliberately NOT flagged — broken signal in 2026 and Boris-style British English uses them. `verdict` is `clean | suspicious | likely_ai` by confidence-weighted density per 1000 words.
- **`information_gain`** — `{ score: 0..10, counts, samples[] }`. Google's March-2026 Core Update made Information Gain a dominant content-quality signal. Counts named-source quotes, sample-size disclosures (`n=…`), year-over-year deltas, first-person evidence (`we analysed…`), methodology disclosure, numbered citations. Below 5 starts deducting from the score; below 2 hits hard.
- **`metatext`** — `{ signals[], heading_skeleton: { jaccard, matched[] }, weighted_score_per_1000_words, verdict }`. Catches the *agent-speaking-instead-of-content* class of slop the lexical detector misses: process narration ("I'll start by…"), self-identification ("As an AI…"), closing pleasantries ("Hope this helps"), bracket asides, markdown envelopes, hedge stacks, sycophancy ("Great question!"). Plus a novel **heading-skeleton Jaccard detector** — flags pages whose outline matches the canonical AI table-of-contents (Introduction / Background / Key Features / FAQ / Conclusion). Position-weighted: openers in the first 15% and pleasantries in the last 20% weigh more.
- **`copy_precision`** — `{ score: 0..10, counts, densities, verdict }`. Positive score. Velvet-glove discipline: rewards tight prose, penalises filler words (`very`, `really`, `actually`), `-ly` adverbs, hedged modals, empty-emphasis adjectives (`crucial`, `key`, `vital`), throat-clearing openers, low sentence-length variance, long Latinate words, absent concrete nouns. Verdicts `tight` (≥8), `mid` (5..7), `padded` (<5), or `insufficient_content` when the body is too short (<20 words) to assess honestly.
- **`design_slop`** — `{ findings[], counts, verdict }`. Visual / typographic / colour tells of AI-generated UIs in 2026. Catches the indigo-CTA cluster, gradient text, the AI purple/violet palette, `cubic-bezier` overshoot bouncing, monotonous spacing, `everything-centered`, single-font pages, the overused-font list (Inter, Geist, Fraunces, Plus Jakarta Sans, etc.), shadcn's unmodified `:root` palette, next-forge defaults, and the **2026 Claude/Anthropic Fraunces + warm-brown italic** wave. Pattern bank ported from Paul Bakaus's [`pbakaus/impeccable`](https://github.com/pbakaus/impeccable) (Apache-2.0) — see `NOTICE`.
- `position_bias` — word-offset percentages of TL;DR / first stat / first credential, with warnings when high-leverage signals sit past the first 30% of the page
- `freshness` — `dateModified`, `datePublished`, days since last modification, year mentions, plus the first `<time datetime>` value, the visible "Updated …" label, and a `schema_vs_visible_severity` (`none`/`mild`/`severe`) that flags pages whose JSON-LD claims a fresher date than the visible text
- **`performance`** — `{ inline_style_bytes, inline_script_bytes, external_stylesheets, external_scripts, render_blocking_estimate, lazy_loaded_images, eager_below_fold_estimate, font_links }`. Markup-only Core Web Vitals proxies — no headless browser needed.
- **`link_graph`** — `{ internal_count, external_count, authority_external, broken_anchors[], nofollow_external_pct }`. Catches orphan pages, missing authority citations, broken in-page anchors.
- `score` — rough 0–100, weighted toward AI-citation surface
- **`score_breakdown`** — per-component deductions: `{ name, deducted, reason }[]`
- `suggestions` — flat list of concrete next actions

### Output formats

| `--out` extension | What you get | Use case |
|---|---|---|
| `.json` | Pretty JSON envelope | Programmatic, default |
| `.html` | Self-contained printable report, serif typography, no JS, no CDN | Share with a client, print, attach to a PR |
| `.sarif` | SARIF 2.1.0 | GitHub Code Scanning annotations |

### Closing the loop with `verify`

LLMs and coding agents often claim work they did not finish. `verify` is the gate:

```bash
aiseo audit page.html --out before.json
# ...agent edits page.html...
aiseo verify before.json page.html
```

Returns a typed diff:

```json
{
  "previous": { "score": 60 },
  "current":  { "score": 92 },
  "delta": {
    "score_change": 32,
    "fixed":         ["og:title absent.", "dateModified absent. ..."],
    "regressed":     [],
    "still_present": []
  },
  "verdict": "pass"
}
```

Exit code is `1` if anything is still present or a new suggestion regressed. The audit data is on stdout regardless — the gate only flips the exit code.

## Why this exists

The Python skill at [`claude-skill-seo-geo-optimizer`](https://github.com/199-biotechnologies/claude-skill-seo-geo-optimizer) was a 13-script bundle stitched together with regex and `urllib`. It worked, but the install footprint was big, the parsing was brittle, and it was ergonomically wrong for an agent: too many subcommands, too much documentation in the skill file, and no way to call it without Python on the box.

`aiseo` collapses the same surface into a single binary, parses HTML with `scraper` instead of regex, returns honest typed JSON, and ships zero documentation in the skill — the binary describes itself.

## Research basis (July 2026)

The audit's heuristics are grounded in the post-Gemini-3 AI search landscape, re-verified against the mid-2026 literature:

- **Position bias** — first ~30% of a page captures ~44% of AI-search citations (iPullRank, AIBoost 2026). Vemetric 2026 adds: a direct answer within the first ~200 words measurably raises citation odds.
- **Answer-first over "TL;DR"** — what's evidenced is the early answer, not the label. Labelled summary blocks (TL;DR, Key takeaways) help machine extraction (Animalz 2026) but are optional style; the audit detects the wider marker family and scores placement, not the label.
- **Ranking ≠ citation** — pages ranking top-10 organically supplied 76% of AI Overview citations in July 2025, 38% by March 2026 (digitalapplied). Classic rank is no longer a proxy for AI visibility.
- **Word count** — Ahrefs' 174k-page study found ~zero correlation (0.04) between length and AI citation. Only genuinely thin pages (<300 words) are flagged; there is no ideal-length check.
- **Open Graph as AI metadata** — the tags that power link previews now power citation cards in ChatGPT/Perplexity/Claude surfaces. 1200×630 (1.91:1) remains the universal size; declared `og:image:width/height` avoid the imageless first share; relative URLs fail silently everywhere.
- **llms.txt — deliberately not checked** — no major provider (OpenAI, Anthropic, Google, Meta) has committed to reading it; Ahrefs' 137k-site study found 97% of llms.txt files receive zero AI-crawler requests. `aiseo` will not tell you to add one.
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
