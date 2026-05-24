//! Design-slop detector.
//!
//! Catches the visual / typographic / colour anti-patterns that have become
//! the canonical fingerprint of AI-generated UIs in 2026 — purple/violet
//! gradients, the indigo CTA, gradient text, bounce easing, monotonous
//! spacing, the overused-font cluster (Inter, Geist, Fraunces, etc.), the
//! shadcn-default `:root` token block, and the Vercel next-forge defaults.
//!
//! Patterns ported (with adaptations) from Paul Bakaus's `pbakaus/impeccable`
//! at https://github.com/pbakaus/impeccable (Apache-2.0, 29.7k stars,
//! commit-date 2026-05-22) — specifically
//! `cli/engine/engines/regex/detect-text.mjs` and
//! `cli/engine/shared/constants.mjs`. See NOTICE for attribution.
//!
//! Additions beyond impeccable:
//!   - `claude_fraunces_wave`     the 2026 Anthropic frontend-design tell
//!   - `shadcn_default_oklch`     unmodified shadcn `:root` palette
//!   - `vercel_next_forge_default` next-forge + geist + shadcn combo

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct DesignSlop {
    pub findings: Vec<Finding>,
    pub counts: HashMap<&'static str, usize>,
    pub verdict: &'static str,
}

#[derive(Serialize, Clone)]
pub struct Finding {
    pub id: &'static str,
    pub snippet: String,
}

struct Matcher {
    id: &'static str,
    re: Regex,
    /// Returns `Some(snippet)` if the match passes the rule's contextual
    /// check, `None` to skip. Receives the full match and the line it
    /// appears in (impeccable's pattern).
    pass: fn(&regex::Captures, &str) -> Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers (the impeccable line-level predicates, transliterated)
// ---------------------------------------------------------------------------

fn has_rounded(line: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\brounded(?:-\w+)?\b").unwrap());
    RE.is_match(line)
}

fn has_border_radius(line: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)border-radius").unwrap());
    RE.is_match(line)
}

fn is_safe_element(line: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?i)<(?:blockquote|nav[\s>]|pre[\s>]|code[\s>]|a\s|input[\s>]|span[\s>])")
            .unwrap()
    });
    RE.is_match(line)
}

// ---------------------------------------------------------------------------
// Per-line regex matchers
// ---------------------------------------------------------------------------

static MATCHERS: Lazy<Vec<Matcher>> = Lazy::new(|| {
    fn mk(id: &'static str, re: &str, pass: fn(&regex::Captures, &str) -> Option<String>) -> Matcher {
        Matcher {
            id,
            re: Regex::new(re).unwrap_or_else(|e| panic!("design_slop regex `{id}`: {e}")),
            pass,
        }
    }

    vec![
        // ── side-tab (Tailwind border-l/r-N) ─────────────────────────────────
        mk(
            "side-tab",
            r"\bborder-[lrse]-(\d+)\b",
            |c, line| {
                let n: u32 = c[1].parse().ok()?;
                let trip = if has_rounded(line) { n >= 1 } else { n >= 4 };
                if trip { Some(c[0].to_string()) } else { None }
            },
        ),
        // ── side-tab (CSS border-left/right: Npx solid) ──────────────────────
        mk(
            "side-tab",
            r"(?i)border-(?:left|right)-width\s*:\s*(\d+)px",
            |c, line| {
                if is_safe_element(line) {
                    return None;
                }
                let n: u32 = c[1].parse().ok()?;
                if n >= 3 { Some(c[0].to_string()) } else { None }
            },
        ),
        // ── border-accent-on-rounded ─────────────────────────────────────────
        mk(
            "border-accent-on-rounded",
            r"\bborder-[tb]-(\d+)\b",
            |c, line| {
                if !has_rounded(line) {
                    return None;
                }
                let n: u32 = c[1].parse().ok()?;
                if n >= 1 { Some(c[0].to_string()) } else { None }
            },
        ),
        mk(
            "border-accent-on-rounded",
            r"(?i)border-(?:top|bottom)\s*:\s*(\d+)px\s+solid",
            |c, line| {
                let n: u32 = c[1].parse().ok()?;
                if n >= 3 && has_border_radius(line) {
                    Some(c[0].to_string())
                } else {
                    None
                }
            },
        ),
        // ── overused-font (CSS font-family) ──────────────────────────────────
        mk(
            "overused-font",
            r#"(?i)font-family\s*:\s*['"]?(Inter|Roboto|Open Sans|Lato|Montserrat|Arial|Helvetica|Fraunces|Geist Sans|Geist Mono|Geist|Mona Sans|Plus Jakarta Sans|Space Grotesk|Recoleta|Instrument Sans)\b"#,
            |c, _| Some(c[0].to_string()),
        ),
        // ── overused-font (Google Fonts link) ────────────────────────────────
        mk(
            "overused-font",
            r"(?i)fonts\.googleapis\.com/css2?\?family=(Inter|Roboto|Open\+Sans|Lato|Montserrat|Fraunces|Plus\+Jakarta\+Sans|Space\+Grotesk|Instrument\+Sans|Mona\+Sans|Geist)\b",
            |c, _| Some(format!("Google Fonts: {}", c[1].replace('+', " "))),
        ),
        // ── claude_fraunces_wave: the 2026 Anthropic tell — Fraunces + italic
        //    headings + warm-brown editorial palette (#3-#4 from X chatter)
        mk(
            "claude_fraunces_wave",
            r#"(?i)font-family\s*:\s*['"]?(Fraunces|Recoleta|Newsreader|Tiempos)\b"#,
            |c, line| {
                let italic = Regex::new(r"(?i)font-style\s*:\s*italic|\bitalic\b").unwrap();
                let warm = Regex::new(r"#(?:7c2d12|92400e|9a3412|c2410c|d97706|b45309|78350f|fef3c7|fed7aa|fffbeb)\b").unwrap();
                if italic.is_match(line) || warm.is_match(line) {
                    Some(format!("{} + italic / warm palette", &c[1]))
                } else {
                    None
                }
            },
        ),
        // ── pure-black-white (CSS) ───────────────────────────────────────────
        mk(
            "pure-black-white",
            r"(?i)background(?:-color)?\s*:\s*(#000000|#000|rgb\(\s*0\s*,\s*0\s*,\s*0\s*\))\b",
            |c, _| Some(c[0].to_string()),
        ),
        // ── pure-black-white (Tailwind bg-black) ─────────────────────────────
        mk(
            "pure-black-white",
            r"\bbg-black\b",
            |c, _| Some(c[0].to_string()),
        ),
        // ── gradient-text (CSS) ──────────────────────────────────────────────
        mk(
            "gradient-text",
            r"(?i)background-clip\s*:\s*text|-webkit-background-clip\s*:\s*text",
            |_, line| {
                if Regex::new(r"(?i)gradient").unwrap().is_match(line) {
                    Some("background-clip: text + gradient".to_string())
                } else {
                    None
                }
            },
        ),
        // ── gradient-text (Tailwind) ─────────────────────────────────────────
        mk(
            "gradient-text",
            r"\bbg-clip-text\b",
            |_, line| {
                if Regex::new(r"(?i)\bbg-gradient-to-").unwrap().is_match(line) {
                    Some("bg-clip-text + bg-gradient".to_string())
                } else {
                    None
                }
            },
        ),
        // ── gray-on-color (Tailwind) ─────────────────────────────────────────
        mk(
            "gray-on-color",
            r"\btext-(?:gray|slate|zinc|neutral|stone)-(\d+)\b",
            |c, line| {
                let bgre = Regex::new(
                    r"\bbg-(?:red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+\b",
                )
                .unwrap();
                bgre.find(line).map(|bg| format!("{} on {}", &c[0], bg.as_str()))
            },
        ),
        // ── ai-color-palette (Tailwind purple/violet/indigo on heading) ──────
        mk(
            "ai-color-palette",
            r"\btext-(?:purple|violet|indigo)-(\d+)\b",
            |c, line| {
                if Regex::new(r"(?i)\btext-(?:[2-9]xl|[3-9]xl)\b|<h[1-3]").unwrap().is_match(line) {
                    Some(format!("{} on heading", &c[0]))
                } else {
                    None
                }
            },
        ),
        // ── ai-color-palette (Tailwind purple→cyan-ish gradient) ─────────────
        mk(
            "ai-color-palette",
            r"\bfrom-(?:purple|violet|indigo)-(\d+)\b",
            |c, line| {
                if Regex::new(
                    r"\bto-(?:purple|violet|indigo|blue|cyan|pink|fuchsia)-\d+\b",
                )
                .unwrap()
                .is_match(line)
                {
                    Some(format!("{} gradient", &c[0]))
                } else {
                    None
                }
            },
        ),
        // ── ai-color-palette (purple/violet hex in gradient or text) ─────────
        mk(
            "ai-color-palette",
            r"(?i)#(7c3aed|8b5cf6|a855f7|9333ea|7e22ce|6d28d9|6366f1|764ba2|667eea)\b",
            |c, _| Some(format!("#{}", &c[1])),
        ),
        // ── bounce-easing (Tailwind animate-bounce) ──────────────────────────
        mk(
            "bounce-easing",
            r"\banimate-bounce\b",
            |_, _| Some("animate-bounce (Tailwind)".to_string()),
        ),
        // ── bounce-easing (CSS named easings) ────────────────────────────────
        mk(
            "bounce-easing",
            r"(?i)animation(?:-name)?\s*:\s*[^;]*\b(bounce|elastic|wobble|jiggle|spring)\b",
            |c, _| Some(c[0].to_string()),
        ),
        // ── bounce-easing (overshooting cubic-bezier control points) ─────────
        mk(
            "bounce-easing",
            r"cubic-bezier\(\s*([-\d.]+)\s*,\s*([-\d.]+)\s*,\s*([-\d.]+)\s*,\s*([-\d.]+)\s*\)",
            |c, _| {
                let y1: f64 = c[2].parse().ok()?;
                let y2: f64 = c[4].parse().ok()?;
                if y1 < -0.1 || y1 > 1.1 || y2 < -0.1 || y2 > 1.1 {
                    Some(format!(
                        "cubic-bezier({}, {}, {}, {})",
                        &c[1], &c[2], &c[3], &c[4]
                    ))
                } else {
                    None
                }
            },
        ),
        // ── layout-transition (CSS transition: width/height/padding/margin) ─
        mk(
            "layout-transition",
            r"(?i)transition\s*:\s*([^;{}]+)",
            |c, _| {
                let val = c[1].to_ascii_lowercase();
                if Regex::new(r"\ball\b").unwrap().is_match(&val) {
                    return None;
                }
                let bad =
                    Regex::new(r"\b(?:(?:max|min)-)?(?:width|height)\b|\bpadding\b|\bmargin\b")
                        .unwrap();
                if bad.is_match(&val) {
                    Some(format!("transition: {}", c[1].trim()))
                } else {
                    None
                }
            },
        ),
        mk(
            "layout-transition",
            r"(?i)transition-property\s*:\s*([^;{}]+)",
            |c, _| {
                let val = c[1].to_ascii_lowercase();
                if Regex::new(r"\ball\b").unwrap().is_match(&val) {
                    return None;
                }
                let bad =
                    Regex::new(r"\b(?:(?:max|min)-)?(?:width|height)\b|\bpadding\b|\bmargin\b")
                        .unwrap();
                if bad.is_match(&val) {
                    Some(format!("transition-property: {}", c[1].trim()))
                } else {
                    None
                }
            },
        ),
        // ── justified-text ──────────────────────────────────────────────────
        mk(
            "justified-text",
            r"(?i)text-align\s*:\s*justify|\btext-justify\b",
            |c, _| Some(c[0].to_string()),
        ),
        // ── tiny-text (CSS) ─────────────────────────────────────────────────
        mk(
            "tiny-text",
            r"(?i)font-size\s*:\s*(1[0-3])(?:px)\b",
            |c, _| Some(format!("font-size: {}px", &c[1])),
        ),
        // ── tight-leading (CSS) ─────────────────────────────────────────────
        mk(
            "tight-leading",
            r"(?i)line-height\s*:\s*(0?\.\d+|1\.[0-2]\d?)\b",
            |c, _| Some(format!("line-height: {}", &c[1])),
        ),
        // ── all-caps-body (Tailwind) ────────────────────────────────────────
        mk(
            "all-caps-body",
            r"<(?:p|li|article)[^>]*\bclass=[^>]*\buppercase\b",
            |c, _| Some(format!("<{}…> with `uppercase` on long-form text", c[0].chars().take(8).collect::<String>())),
        ),
        // ── wide-tracking on body (CSS) ─────────────────────────────────────
        mk(
            "wide-tracking",
            r"(?i)letter-spacing\s*:\s*(0\.0[6-9]|0\.[1-9])(?:em|rem)",
            |c, _| Some(format!("letter-spacing: {}em", &c[1])),
        ),
        // ── shadcn_default_oklch — the unmodified shadcn :root palette ──────
        mk(
            "shadcn_default_oklch",
            r"--radius\s*:\s*0\.625rem",
            |_, line| {
                if Regex::new(r"--background\s*:\s*oklch\(1\s+0\s+0\)")
                    .unwrap()
                    .is_match(line)
                    || Regex::new(r"--muted-foreground\s*:\s*oklch\(0\.556\s+0\s+0\)")
                        .unwrap()
                        .is_match(line)
                    || Regex::new(r"--destructive\s*:\s*oklch\(0\.577\s+0\.245\s+27\.325\)")
                        .unwrap()
                        .is_match(line)
                {
                    Some("shadcn unmodified :root palette".to_string())
                } else {
                    Some("--radius: 0.625rem (likely shadcn)".to_string())
                }
            },
        ),
        // ── vercel_next_forge_default ───────────────────────────────────────
        mk(
            "vercel_next_forge_default",
            r"--font-sans\s*:\s*var\(--font-geist-sans\)",
            |_, _| Some("next-forge default: --font-sans + geist".to_string()),
        ),
    ]
});

// ---------------------------------------------------------------------------
// Doc-level analyzers (aggregate signals impossible to express per-line)
// ---------------------------------------------------------------------------

static GENERIC_FONTS: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
    [
        "serif",
        "sans-serif",
        "monospace",
        "system-ui",
        "ui-sans-serif",
        "ui-serif",
        "ui-monospace",
        "ui-rounded",
        "cursive",
        "fantasy",
        "math",
        "emoji",
        "inherit",
        "initial",
        "unset",
        "revert",
        "revert-layer",
    ]
    .into_iter()
    .collect()
});

fn analyze_single_font(html: &str) -> Option<Finding> {
    let re = Regex::new(r"(?i)font-family\s*:\s*([^;}]+)").unwrap();
    let gf = Regex::new(r#"(?i)fonts\.googleapis\.com/css2?\?family=([^&"'\s]+)"#).unwrap();

    let mut fonts: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in re.captures_iter(html) {
        for f in c[1].split(',') {
            let name = f
                .trim()
                .trim_matches(|ch| ch == '\'' || ch == '"')
                .to_ascii_lowercase();
            if !name.is_empty() && !GENERIC_FONTS.contains(name.as_str()) {
                fonts.insert(name);
            }
        }
    }
    for c in gf.captures_iter(html) {
        for f in c[1].split('|') {
            let name = f
                .split(':')
                .next()
                .unwrap_or("")
                .replace('+', " ")
                .to_ascii_lowercase();
            if !name.is_empty() {
                fonts.insert(name);
            }
        }
    }

    if fonts.len() == 1 && html.lines().count() >= 20 {
        let only = fonts.into_iter().next().unwrap();
        Some(Finding {
            id: "single-font",
            snippet: format!("only font used is {only}"),
        })
    } else {
        None
    }
}

fn analyze_monotonous_spacing(html: &str) -> Option<Finding> {
    let px_re = Regex::new(r"(?i)(?:padding|margin)(?:-(?:top|right|bottom|left))?\s*:\s*(\d+)px").unwrap();
    let rem_re = Regex::new(r"(?i)(?:padding|margin)(?:-(?:top|right|bottom|left))?\s*:\s*([\d.]+)rem").unwrap();
    let gap_re = Regex::new(r"(?i)gap\s*:\s*(\d+)px").unwrap();
    let tw_re = Regex::new(r"\b(?:p|px|py|pt|pb|pl|pr|m|mx|my|mt|mb|ml|mr|gap)-(\d+)\b").unwrap();

    let mut vals: Vec<u32> = Vec::new();
    for c in px_re.captures_iter(html) {
        if let Ok(v) = c[1].parse::<u32>()
            && v > 0
            && v < 200
        {
            vals.push(v);
        }
    }
    for c in rem_re.captures_iter(html) {
        if let Ok(v) = c[1].parse::<f64>() {
            let px = (v * 16.0).round() as u32;
            if px > 0 && px < 200 {
                vals.push(px);
            }
        }
    }
    for c in gap_re.captures_iter(html) {
        if let Ok(v) = c[1].parse::<u32>() {
            vals.push(v);
        }
    }
    for c in tw_re.captures_iter(html) {
        if let Ok(v) = c[1].parse::<u32>() {
            vals.push(v.saturating_mul(4));
        }
    }

    let rounded: Vec<u32> = vals.iter().map(|v| (v / 4) * 4).collect();
    if rounded.len() < 10 {
        return None;
    }
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for v in &rounded {
        *counts.entry(*v).or_insert(0) += 1;
    }
    let max_count = *counts.values().max().unwrap_or(&0);
    let pct = max_count as f64 / rounded.len() as f64;
    let unique: std::collections::HashSet<&u32> =
        rounded.iter().filter(|v| **v > 0).collect();
    if pct > 0.6 && unique.len() <= 3 {
        let dominant = counts
            .iter()
            .max_by_key(|(_, c)| **c)
            .map(|(v, _)| *v)
            .unwrap_or(0);
        Some(Finding {
            id: "monotonous-spacing",
            snippet: format!(
                "~{dominant}px used {max_count}/{} times ({}%)",
                rounded.len(),
                (pct * 100.0).round() as u32
            ),
        })
    } else {
        None
    }
}

fn analyze_everything_centered(html: &str) -> Option<Finding> {
    let line_re = Regex::new(r"(?i)<(?:h[1-6]|p|div|li|button)\b[^>]*>").unwrap();
    let cent_re = Regex::new(r"(?i)text-align\s*:\s*center|\btext-center\b").unwrap();
    let mut centered = 0usize;
    let mut total = 0usize;
    for line in html.lines() {
        if line.trim().len() > 20 && line_re.is_match(line) {
            total += 1;
            if cent_re.is_match(line) {
                centered += 1;
            }
        }
    }
    if total < 5 {
        return None;
    }
    let ratio = centered as f64 / total as f64;
    if ratio > 0.7 {
        Some(Finding {
            id: "everything-centered",
            snippet: format!(
                "{centered}/{total} text elements centered ({}%)",
                (ratio * 100.0).round() as u32
            ),
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn extract(html: &str) -> DesignSlop {
    let mut findings: Vec<Finding> = Vec::new();
    let mut counts: HashMap<&'static str, usize> = HashMap::new();

    for (line_no, line) in html.lines().enumerate() {
        if line_no > 5000 {
            break; // sanity cap on hostile huge pages
        }
        for m in MATCHERS.iter() {
            for cap in m.re.captures_iter(line) {
                if let Some(snippet) = (m.pass)(&cap, line) {
                    *counts.entry(m.id).or_insert(0) += 1;
                    if findings.len() < 50 {
                        findings.push(Finding {
                            id: m.id,
                            snippet,
                        });
                    }
                }
            }
        }
    }

    for f in [
        analyze_single_font(html),
        analyze_monotonous_spacing(html),
        analyze_everything_centered(html),
    ]
    .into_iter()
    .flatten()
    {
        *counts.entry(f.id).or_insert(0) += 1;
        findings.push(f);
    }

    let total: usize = counts.values().sum();
    let verdict = if total == 0 {
        "clean"
    } else if total <= 4 {
        "minor"
    } else if total <= 10 {
        "suspicious"
    } else {
        "heavy"
    };

    DesignSlop {
        findings,
        counts,
        verdict,
    }
}

/// Suggestion text appended to the audit when verdict is not clean.
pub fn suggestion(slop: &DesignSlop) -> Option<String> {
    match slop.verdict {
        "clean" | "minor" => None,
        "suspicious" => Some(format!(
            "Design-slop verdict: suspicious ({} hits across {} kinds). Common 2026 AI tells present.",
            slop.findings.len(),
            slop.counts.len()
        )),
        "heavy" => Some(format!(
            "Design-slop verdict: heavy ({} hits across {} kinds). Reads as templated AI-generated UI; differentiate the palette, fonts, and layout.",
            slop.findings.len(),
            slop.counts.len()
        )),
        _ => None,
    }
}
