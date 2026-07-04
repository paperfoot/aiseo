//! Convert raw findings into a flat list of actionable suggestions plus a
//! 0-100 score. Agents read the `suggestions` array; humans want a quick
//! gut number.

use super::{
    AiSlop, ContentStructure, Freshness, InformationGain, Meta, OpenGraph, PositionBias,
    TwitterCard,
};
use serde::Serialize;

/// Per-component deduction. Agents read this to know *which* axis to fix
/// next, not just the bottom-line score.
#[derive(Serialize)]
pub struct ScoreBreakdown {
    pub total: u32,
    pub components: Vec<ScoreComponent>,
}

#[derive(Serialize)]
pub struct ScoreComponent {
    pub name: &'static str,
    pub deducted: u32,
    pub reason: &'static str,
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    meta: &Meta,
    og: &OpenGraph,
    tw: &TwitterCard,
    content: &ContentStructure,
    pos: &PositionBias,
    fresh: &Freshness,
    schema_types: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // ── Metadata ────────────────────────────────────────────────────────────
    match meta.title.as_deref() {
        None => out.push("Title absent. Aim 50..60 chars.".into()),
        Some(t) if t.chars().count() < 30 => out.push(format!(
            "Title is {} chars. Aim 50..60.",
            t.chars().count()
        )),
        Some(t) if t.chars().count() > 70 => out.push(format!(
            "Title is {} chars. Mobile snippets truncate around 70.",
            t.chars().count()
        )),
        _ => {}
    }

    match meta.description.as_deref() {
        None => out.push("Meta description absent. 150..160 chars.".into()),
        Some(d) if d.chars().count() < 100 => out.push(format!(
            "Meta description is {} chars. Aim 150..160.",
            d.chars().count()
        )),
        Some(d) if d.chars().count() > 170 => out.push(format!(
            "Meta description is {} chars. Snippets truncate past 160.",
            d.chars().count()
        )),
        _ => {}
    }

    // ── Open Graph ──────────────────────────────────────────────────────────
    // OG is no longer just social chrome: the same tags feed link previews
    // in ChatGPT/Perplexity/Claude citations. "Present" is not "working" —
    // the common failure is a tag that exists but resolves to nothing.
    if og.title.is_none() {
        out.push("og:title absent.".into());
    }
    match og.image.as_deref() {
        None => out.push(
            "og:image absent. Shares and AI-citation previews render bare. Ship 1200×630 (1.91:1), absolute https URL.".into(),
        ),
        Some(v) if !(v.starts_with("https://") || v.starts_with("http://")) => out.push(format!(
            "og:image is not an absolute URL (`{v}`). Crawlers resolve it against nothing — the preview silently fails everywhere. Use the full https URL."
        )),
        Some(v) if v.starts_with("http://") => out.push(
            "og:image is plain http. Platforms that refuse mixed content drop the preview — serve it over https.".into(),
        ),
        Some(_) => {
            if og.image_width.is_none() || og.image_height.is_none() {
                out.push(
                    "og:image:width/height absent. The first share of any page renders imageless while the crawler fetches the file — declare 1200 and 630.".into(),
                );
            }
            if og.image_alt.is_none() {
                out.push("og:image:alt absent. Screen readers and multimodal retrieval read it.".into());
            }
        }
    }
    if og.image.is_some() && og.description.is_none() {
        out.push("og:description absent. Platforms fall back to the meta description; AI previews may show arbitrary page text instead.".into());
    }
    if let (Some(og_url), Some(canon)) = (og.url.as_deref(), meta.canonical.as_deref())
        && og_url.trim_end_matches('/') != canon.trim_end_matches('/')
    {
        out.push(format!(
            "og:url (`{og_url}`) and canonical (`{canon}`) disagree. Shares and citations split across two URLs — align them."
        ));
    }
    if og.og_type.is_none() && is_article(schema_types) {
        out.push("og:type absent on an Article page. Declare `og:type=article`.".into());
    }
    // twitter:card is the one twitter:* tag OG can't replace: X falls back
    // to og:title/og:image per-field, but without the card type it renders
    // the small summary card, not the large image.
    if tw.card.is_none() && og.image.is_some() {
        out.push(
            "twitter:card absent. X falls back to OG for text and image but renders the small card — add `twitter:card=summary_large_image`.".into(),
        );
    }

    if meta.canonical.is_none() {
        out.push(
            "Canonical link absent. Without it, near-duplicate URLs dilute Google's index and confuse downstream retrieval.".into(),
        );
    }

    // Robots meta — noindex/nofollow are the actual findings, not absence.
    if let Some(r) = meta.robots.as_deref() {
        let lower = r.to_ascii_lowercase();
        if lower.contains("noindex") {
            out.push(format!(
                "Robots meta is `{}`. Page is excluded from search and AI retrieval.",
                r
            ));
        } else if lower.contains("nofollow") {
            out.push(format!(
                "Robots meta is `{}`. Links on this page pass no authority.",
                r
            ));
        }
    }

    // Viewport — absence breaks mobile rendering and hurts mobile-first
    // indexing.
    if meta.viewport.is_none() {
        out.push(
            "Viewport meta absent. Mobile rendering breaks; Google indexes mobile-first.".into(),
        );
    }

    // ── Content structure ────────────────────────────────────────────────────
    if content.h1.is_empty() {
        out.push("H1 absent.".into());
    } else if content.h1.len() > 1 {
        out.push(format!(
            "{} H1s. Use one; demote the rest.",
            content.h1.len()
        ));
    }
    if content.h2.len() < 2 {
        out.push(format!(
            "{} H2s in main content. H2s act as passage anchors for AI retrieval; one or zero H2s makes the page hard to chunk.",
            content.h2.len()
        ));
    }
    // Word-count thresholds. Google's May-2026 AI optimisation guide
    // (developers.google.com/search/docs/fundamentals/ai-optimization-guide)
    // explicitly says there's no ideal length and no required chunk size.
    // Only flag pages that are *too thin* to satisfy a question, not pages
    // that are "wrong length" by some made-up rule.
    if content.word_count < 300 {
        out.push(format!(
            "Body is {} words. Thin pages rarely satisfy substantive queries.",
            content.word_count
        ));
    }
    // Answer-first: what's evidenced is answering the query early (Vemetric
    // 2026: a direct answer in the first ~200 words raises citation odds).
    // A "TL;DR:" label is one way to mark it, not the requirement — labelled
    // summary blocks help machine extraction (Animalz 2026) but a label-less
    // opening paragraph that states the answer does the same job.
    if !content.has_tldr && content.word_count >= 150 {
        out.push(
            "No answer-first summary detected. State the core answer in the opening 40..60 words; a label (TL;DR, Key takeaways) helps extraction but is optional.".into(),
        );
    }
    if !content.has_credentials && content.has_author && is_english(content) {
        // Recast (Codex 2026-05-24): the "lifts ChatGPT/Claude citation"
        // formulation was stronger than any current public source supports.
        // Credentials matter for entity disambiguation and reviewer trust,
        // which is downstream of ranking, not a direct lift.
        out.push("Author byline present but no visible credentials. Credentials help entity disambiguation and reviewer trust on YMYL pages.".into());
    }
    if content.missing_alt_count > 0 {
        out.push(format!(
            "{} images missing alt text. Multimodal AI search reads alt.",
            content.missing_alt_count
        ));
    }
    if content.html_lang.is_none() {
        out.push("`<html lang>` absent. Multilingual AI retrieval relies on it.".into());
    }

    // ── Schema ───────────────────────────────────────────────────────────────
    if schema_types.is_empty() {
        out.push("No JSON-LD. Article + Organization at minimum; FAQ for question pages.".into());
    }
    // FAQPage schema is still valid; Google retired the SERP rich result
    // on 7 May 2026 but the schema itself is fine. Safe to keep on
    // FAQ-heavy pages, just redundant on light ones.
    if schema_types.iter().any(|t| t == "FAQPage") && content.word_count < 800 {
        out.push(
            "FAQPage schema no longer earns a Google rich result (retired 7 May 2026). Keep it on FAQ-heavy pages — Bing and AI platforms still read it — but it's dead weight on a page this light."
                .into(),
        );
    }
    // Schema density. The negative-lift-past-6 claim previously cited
    // here lacked a verifiable primary source — Ahrefs Apr-2026 reported
    // schema correlated with +2.4% AI Mode citation (noise), but never
    // published a past-6 cliff. Reframed as a maintenance-burden note,
    // not a ranking penalty.
    if schema_types.len() > 8 {
        out.push(format!(
            "{} JSON-LD @types on one page. Past ~8 distinct types the maintenance burden outweighs the ranking signal — collapse overlapping types (Product+Offer+Service, etc.).",
            schema_types.len()
        ));
    }

    // ── Noscript / hreflang / heading hierarchy ────────────────────────────
    if content.noscript_kind == crate::audit::content::NoscriptKind::BoilerplateOnly {
        out.push(
            "`<noscript>` only says \"enable JavaScript\". Crawlers see nothing on JS-only pages."
                .into(),
        );
    }
    // Heading-order violations: H3 before any H2, H2 before any H1,
    // levels skipped (h1 -> h3 directly). Chrome headings (nav/footer)
    // are filtered upstream so footer columns no longer trigger this.
    if let Some(v) = heading_hierarchy_violation(&content.headings_in_order) {
        out.push(v);
    }
    if content.duplicate_heading_count > 0 {
        out.push(format!(
            "{} duplicate heading{}. Headings are passage anchors for AI retrieval; identical text confuses fan-out.",
            content.duplicate_heading_count,
            if content.duplicate_heading_count == 1 { "" } else { "s" },
        ));
    }
    if content.empty_heading_count > 0 {
        out.push(format!(
            "{} empty heading tag{}. Delete or fill — empty headings break outline navigation.",
            content.empty_heading_count,
            if content.empty_heading_count == 1 { "" } else { "s" },
        ));
    }
    // Comparison tables — Perplexity Pages and AI Mode lift content
    // with structured tables. Only flag for medium-or-longer pages.
    if content.table_count == 0 && content.word_count >= 800 {
        out.push(
            "No `<table>` elements. Perplexity and AI Mode lift pages with comparison tables on listy content.".into(),
        );
    }
    // Direct-quotable sentences (5..25 words, complete-sentence shape) —
    // proxy for what assistants cite verbatim. Cold pages with <5 are
    // hard to quote.
    if content.word_count >= 400 && content.quotable_sentence_count < 5 {
        out.push(format!(
            "Only {} quotable sentence{} (5..25 words). AI assistants cite short complete sentences verbatim — add a few standalone claims.",
            content.quotable_sentence_count,
            if content.quotable_sentence_count == 1 { "" } else { "s" },
        ));
    }
    // Hreflang is only flagged if the page declares a non-default lang
    // but advertises no alternates — we don't badger English-only pages.
    if let Some(l) = &content.html_lang
        && !l.starts_with("en")
        && content.hreflangs.is_empty()
    {
        out.push(
            "Non-English page with no `<link rel=alternate hreflang>` alternates. AI engines down-rank lone translations.".into(),
        );
    }

    // ── Freshness ────────────────────────────────────────────────────────────
    if fresh.date_modified.is_none() && is_article(schema_types) {
        out.push(
            "dateModified absent on Article schema. Freshness signals matter on news, evolving topics, and any page where the user reasonably asks \"is this current?\"".into(),
        );
    } else if let Some(days) = fresh.days_since_modified
        && days > 90
    {
        out.push(format!(
            "Last modified {} days ago. If the topic moves, refresh and update dateModified.",
            days
        ));
    }

    // ── Position bias ────────────────────────────────────────────────────────
    out.extend(pos.warnings.iter().cloned());

    out
}

fn heading_hierarchy_violation(
    headings: &[crate::audit::content::HeadingOrderEntry],
) -> Option<String> {
    let mut seen_max: u8 = 0;
    let mut last: u8 = 0;
    for h in headings {
        if h.level > 1 && seen_max == 0 {
            return Some(format!(
                "First heading is H{}, not H1. Crawlers and screen readers rely on H1 first.",
                h.level
            ));
        }
        if last != 0 && h.level > last + 1 {
            return Some(format!(
                "Heading hierarchy skips: H{} after H{}. Use consecutive levels.",
                h.level, last
            ));
        }
        last = h.level;
        if h.level > seen_max {
            seen_max = h.level;
        }
    }
    None
}

fn is_article(schema_types: &[String]) -> bool {
    schema_types.iter().any(|t| {
        matches!(
            t.as_str(),
            "Article" | "NewsArticle" | "BlogPosting" | "ScholarlyArticle" | "TechArticle"
        )
    })
}

fn is_english(content: &ContentStructure) -> bool {
    match &content.html_lang {
        Some(l) => l.starts_with("en"),
        None => true, // unknown → assume English (matches existing default)
    }
}

/// Single source of truth for scoring. `score_breakdown` builds on top.
// Args are the distinct audit sub-results, produced separately by their
// extractors; threading them individually is clearer than an artificial aggregate.
#[allow(clippy::too_many_arguments)]
fn deductions(
    meta: &Meta,
    og: &OpenGraph,
    tw: &TwitterCard,
    content: &ContentStructure,
    pos: &PositionBias,
    fresh: &Freshness,
    ai_slop: &AiSlop,
    info_gain: &InformationGain,
    schema_types: &[String],
) -> Vec<ScoreComponent> {
    let mut out: Vec<ScoreComponent> = Vec::new();
    if meta.title.is_none() {
        out.push(ScoreComponent {
            name: "meta_title",
            deducted: 15,
            reason: "Missing <title>",
        });
    }
    if meta.description.is_none() {
        out.push(ScoreComponent {
            name: "meta_description",
            deducted: 10,
            reason: "Missing meta description",
        });
    }
    if og.title.is_none() {
        out.push(ScoreComponent {
            name: "og_title",
            deducted: 5,
            reason: "Missing og:title",
        });
    }
    match og.image.as_deref() {
        None => out.push(ScoreComponent {
            name: "og_image",
            deducted: 10,
            reason: "Missing og:image",
        }),
        // Relative URL = functionally missing: no platform resolves it.
        Some(v) if !(v.starts_with("https://") || v.starts_with("http://")) => {
            out.push(ScoreComponent {
                name: "og_image_url",
                deducted: 8,
                reason: "og:image is not an absolute URL",
            });
        }
        Some(_) => {
            if og.image_width.is_none() || og.image_height.is_none() {
                out.push(ScoreComponent {
                    name: "og_image_dimensions",
                    deducted: 2,
                    reason: "og:image:width/height not declared",
                });
            }
        }
    }
    if tw.card.is_none() && og.image.is_some() {
        out.push(ScoreComponent {
            name: "twitter_card",
            deducted: 2,
            reason: "Missing twitter:card",
        });
    }
    if content.h1.is_empty() {
        out.push(ScoreComponent {
            name: "h1",
            deducted: 10,
            reason: "No H1 heading",
        });
    }
    if content.h2.len() < 2 {
        out.push(ScoreComponent {
            name: "h2_count",
            deducted: 5,
            reason: "Fewer than 2 H2 headings",
        });
    }
    if content.word_count < 300 {
        out.push(ScoreComponent {
            name: "word_count",
            deducted: 10,
            reason: "Body under 300 words",
        });
    }
    // Kept at 2: the widely-cited "+35% TL;DR lift" was one agency blog
    // with no methodology, since debunked. What holds is answer-first
    // placement (Vemetric 2026) — this component detects the labelled
    // marker only, so it stays a nudge, not a penalty.
    if !content.has_tldr && content.word_count >= 150 {
        out.push(ScoreComponent {
            name: "tldr",
            deducted: 2,
            reason: "No answer-first summary marker",
        });
    }
    // Schema deduction dropped from 15 -> 5 per Ahrefs Apr 2026 study
    // (1,885 pages, +2.4% AI Mode citation lift = noise).
    if schema_types.is_empty() {
        out.push(ScoreComponent {
            name: "schema",
            deducted: 5,
            reason: "No JSON-LD schema",
        });
    }
    if fresh.date_modified.is_none() && is_article(schema_types) {
        out.push(ScoreComponent {
            name: "date_modified",
            deducted: 5,
            reason: "Missing dateModified on Article",
        });
    } else if let Some(days) = fresh.days_since_modified
        && days > 180
    {
        out.push(ScoreComponent {
            name: "staleness",
            deducted: 5,
            reason: "Content >180 days old",
        });
    }
    // Position bias now affects the score — was suggestion-only in v0.3.
    if let Some(p) = pos.tldr_position_pct
        && p > 10.0
    {
        out.push(ScoreComponent {
            name: "tldr_position",
            deducted: 5,
            reason: "Answer summary past first 10% of body",
        });
    }
    if let Some(p) = pos.first_stat_position_pct
        && p > 30.0
    {
        out.push(ScoreComponent {
            name: "first_stat_position",
            deducted: 5,
            reason: "First statistic past first 30% of body",
        });
    }
    if content.missing_alt_count > 0 {
        out.push(ScoreComponent {
            name: "img_alt",
            deducted: 5,
            reason: "Images missing alt text",
        });
    }
    // AI-slop bites the score when the verdict is bad.
    match ai_slop.verdict {
        "suspicious" => out.push(ScoreComponent {
            name: "ai_slop",
            deducted: 5,
            reason: "AI-writing fingerprint suspicious",
        }),
        "likely_ai" => out.push(ScoreComponent {
            name: "ai_slop",
            deducted: 15,
            reason: "AI-writing fingerprint heavy",
        }),
        _ => {}
    }
    // Information Gain: only deduct for pages long enough to plausibly
    // carry evidence. Information Gain is an SEO-community frame (Indig /
    // Search Engine Land), not a Google-acknowledged ranking signal. The
    // patent (US20200349181A1) exists but Google has never confirmed it
    // weighs ranking. Cap the max deduction at 10 (was 15) to reflect that
    // uncertainty — below 2 is a real penalty, 2..4 is a soft penalty.
    if content.word_count >= 300 {
        match info_gain.score {
            0..=1 => out.push(ScoreComponent {
                name: "information_gain",
                deducted: 10,
                reason: "Low Information Gain (rewritten / templated)",
            }),
            2..=4 => out.push(ScoreComponent {
                name: "information_gain",
                deducted: 5,
                reason: "Below the 5..7 competitive band",
            }),
            _ => {}
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn score_breakdown(
    meta: &Meta,
    og: &OpenGraph,
    tw: &TwitterCard,
    content: &ContentStructure,
    pos: &PositionBias,
    fresh: &Freshness,
    ai_slop: &AiSlop,
    info_gain: &InformationGain,
    schema_types: &[String],
) -> ScoreBreakdown {
    let components =
        deductions(meta, og, tw, content, pos, fresh, ai_slop, info_gain, schema_types);
    let total_deducted: u32 = components.iter().map(|c| c.deducted).sum();
    let total = 100u32.saturating_sub(total_deducted);
    ScoreBreakdown { total, components }
}
