//! Static-HTML performance signals.
//!
//! aiseo does not run a headless browser, so Core Web Vitals (LCP / CLS /
//! INP) are out of scope. What we CAN measure from the raw HTML alone:
//! image lazy-loading discipline, declared width/height (CLS proxy),
//! render-blocking script patterns, font and resource hint presence,
//! inline-bundle sizes, and adjacent format hints (`<picture>`/`srcset`).
//!
//! All checks deterministic, no network calls. Pairs with Tier-2 fetch
//! analysis (response size, compression headers) for a complete picture.

use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use serde::Serialize;

#[derive(Serialize)]
pub struct Performance {
    pub images: ImageSignals,
    pub render_blocking: RenderBlocking,
    pub resource_hints: ResourceHints,
    pub fonts: FontSignals,
    pub inline_bytes: InlineBundleBytes,
}

#[derive(Serialize)]
pub struct ImageSignals {
    pub total: usize,
    /// Images below the first ~2 in DOM order that are NOT marked
    /// `loading="lazy"`. The first ~2 are presumably above-the-fold and
    /// should NOT be lazy (lazy-loading the LCP image hurts LCP).
    pub eligible_for_lazy_missing: usize,
    /// Images without BOTH `width` and `height` attributes set —
    /// guaranteed to contribute to Cumulative Layout Shift.
    pub missing_dimensions: usize,
    /// Images whose filename looks templated/non-descriptive
    /// (`IMG_1234.jpg`, `DSC00012.JPG`, `image-1.png`, `photo.jpg`).
    pub non_descriptive_filenames: usize,
    /// Images served via `<picture>` with at least one modern-format
    /// `<source type="image/webp|avif">` sibling.
    pub modern_format_via_picture: usize,
    /// `src` URLs that explicitly point to a modern format.
    pub modern_format_via_src: usize,
}

#[derive(Serialize)]
pub struct RenderBlocking {
    /// `<script src=…>` in `<head>` without `async` or `defer`. These
    /// block rendering until fetched + parsed.
    pub head_scripts_blocking: usize,
    /// External `<link rel="stylesheet">` in `<head>`. Stylesheets are
    /// always render-blocking unless `media` excludes the current viewport.
    pub head_stylesheets: usize,
}

#[derive(Serialize)]
pub struct ResourceHints {
    pub preload: usize,
    pub preconnect: usize,
    pub dns_prefetch: usize,
    pub modulepreload: usize,
    /// True when at least one `<link rel="preload" as="image">` exists.
    /// Critical for LCP-image promotion.
    pub preloads_an_image: bool,
}

#[derive(Serialize)]
pub struct FontSignals {
    /// External font `<link>` count (rel contains preload+as=font OR
    /// stylesheet with a known font host).
    pub external_link_count: usize,
    /// True when ≥1 `@font-face` block declares `font-display: swap` /
    /// `optional` / `fallback`. Absence is a finding on font-heavy pages.
    pub has_font_display_strategy: bool,
    /// Count of `<link rel="preload" as="font">`.
    pub preloaded: usize,
}

#[derive(Serialize)]
pub struct InlineBundleBytes {
    pub inline_css: usize,
    pub inline_js: usize,
}

static NONDESCRIPTIVE_FILENAME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(^|/)(img[_-]?\d+|dsc[_-]?\d+|image[_-]?\d+|photo[_-]?\d+|picture[_-]?\d+|screen ?shot|screenshot)([_-]?\d+)?\.(jpe?g|png|gif|webp|avif|heic)$",
    )
    .unwrap()
});

static MODERN_FORMAT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\.(webp|avif|jxl)(\?|#|$)").unwrap());

static FONT_DISPLAY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)font-display\s*:\s*(swap|optional|fallback)").unwrap());

const FOLD_BUFFER: usize = 2;

pub fn extract(doc: &Html, raw_html: &str) -> Performance {
    Performance {
        images: image_signals(doc),
        render_blocking: render_blocking(doc),
        resource_hints: resource_hints(doc),
        fonts: font_signals(doc, raw_html),
        inline_bytes: inline_bytes(doc),
    }
}

fn image_signals(doc: &Html) -> ImageSignals {
    let sel = Selector::parse("img").unwrap();
    // Codex 2026-05-24: the previous selector matched ANY image/* source
    // including JPEG/PNG — counting legacy formats as "modern". Restrict
    // to the actually-modern set (webp, avif, jxl).
    let picture_sel = Selector::parse(
        "picture > source[type=\"image/webp\"], picture > source[type=\"image/avif\"], picture > source[type=\"image/jxl\"]",
    )
    .unwrap();

    let imgs: Vec<_> = doc.select(&sel).collect();
    let total = imgs.len();

    // Eligible-for-lazy: every image past the first FOLD_BUFFER.
    let mut eligible_missing = 0;
    let mut missing_dims = 0;
    let mut non_descriptive = 0;
    let mut modern_src = 0;

    for (idx, el) in imgs.iter().enumerate() {
        let loading = el.value().attr("loading").unwrap_or("").to_ascii_lowercase();
        let width = el.value().attr("width").unwrap_or("").trim();
        let height = el.value().attr("height").unwrap_or("").trim();
        let src = el.value().attr("src").unwrap_or("");

        if idx >= FOLD_BUFFER && loading != "lazy" {
            eligible_missing += 1;
        }
        if width.is_empty() || height.is_empty() {
            missing_dims += 1;
        }
        if NONDESCRIPTIVE_FILENAME.is_match(src) {
            non_descriptive += 1;
        }
        if MODERN_FORMAT.is_match(src) {
            modern_src += 1;
        }
    }

    // Count `<source>` elements declaring image/webp etc inside `<picture>`.
    // We don't try to match them back to <img>; a non-zero count means
    // the page does at least attempt modern formats.
    let modern_picture = doc.select(&picture_sel).count();

    ImageSignals {
        total,
        eligible_for_lazy_missing: eligible_missing,
        missing_dimensions: missing_dims,
        non_descriptive_filenames: non_descriptive,
        modern_format_via_picture: modern_picture,
        modern_format_via_src: modern_src,
    }
}

fn render_blocking(doc: &Html) -> RenderBlocking {
    let head_sel = Selector::parse("head").unwrap();
    let head = match doc.select(&head_sel).next() {
        Some(h) => h,
        None => {
            return RenderBlocking {
                head_scripts_blocking: 0,
                head_stylesheets: 0,
            };
        }
    };

    let script_sel = Selector::parse("script[src]").unwrap();
    let mut blocking_scripts = 0;
    for s in head.select(&script_sel) {
        let has_async = s.value().attr("async").is_some();
        let has_defer = s.value().attr("defer").is_some();
        let is_module = s.value().attr("type").is_some_and(|t| t.eq_ignore_ascii_case("module"));
        // type=module is implicitly deferred per HTML spec.
        if !has_async && !has_defer && !is_module {
            blocking_scripts += 1;
        }
    }

    // Codex 2026-05-24: filter out non-render-blocking stylesheets.
    // `media="print"` only loads when printing; `disabled` is dormant;
    // a `media` query that excludes the current viewport does not
    // block — but we can't know the viewport statically, so only the
    // unambiguous cases (print, disabled) are excluded.
    let css_sel = Selector::parse("link[rel=\"stylesheet\"]").unwrap();
    let stylesheets = head
        .select(&css_sel)
        .filter(|el| {
            let media = el.value().attr("media").unwrap_or("").to_ascii_lowercase();
            let disabled = el.value().attr("disabled").is_some();
            !disabled && media != "print"
        })
        .count();

    RenderBlocking {
        head_scripts_blocking: blocking_scripts,
        head_stylesheets: stylesheets,
    }
}

fn resource_hints(doc: &Html) -> ResourceHints {
    let link_sel = Selector::parse("link[rel]").unwrap();
    let mut preload = 0;
    let mut preconnect = 0;
    let mut dns_prefetch = 0;
    let mut modulepreload = 0;
    let mut preloads_image = false;
    for el in doc.select(&link_sel) {
        let rel = el.value().attr("rel").unwrap_or("").to_ascii_lowercase();
        if rel.contains("preload") && !rel.contains("modulepreload") {
            preload += 1;
            if el
                .value()
                .attr("as")
                .is_some_and(|a| a.eq_ignore_ascii_case("image"))
            {
                preloads_image = true;
            }
        }
        if rel.contains("preconnect") {
            preconnect += 1;
        }
        if rel.contains("dns-prefetch") {
            dns_prefetch += 1;
        }
        if rel.contains("modulepreload") {
            modulepreload += 1;
        }
    }
    ResourceHints {
        preload,
        preconnect,
        dns_prefetch,
        modulepreload,
        preloads_an_image: preloads_image,
    }
}

fn font_signals(doc: &Html, raw_html: &str) -> FontSignals {
    let link_sel = Selector::parse("link[rel]").unwrap();
    let mut external = 0;
    let mut preloaded = 0;
    for el in doc.select(&link_sel) {
        let rel = el.value().attr("rel").unwrap_or("").to_ascii_lowercase();
        let href = el.value().attr("href").unwrap_or("");
        let as_attr = el.value().attr("as").unwrap_or("").to_ascii_lowercase();
        let is_font_link =
            (rel.contains("preload") && as_attr == "font") || is_font_host(href);
        if is_font_link {
            external += 1;
        }
        if rel.contains("preload") && as_attr == "font" {
            preloaded += 1;
        }
    }
    let has_strategy = FONT_DISPLAY.is_match(raw_html);
    FontSignals {
        external_link_count: external,
        has_font_display_strategy: has_strategy,
        preloaded,
    }
}

fn is_font_host(href: &str) -> bool {
    let h = href.to_ascii_lowercase();
    h.contains("fonts.googleapis.com")
        || h.contains("fonts.gstatic.com")
        || h.contains("use.typekit.net")
        || h.contains("use.fontawesome.com")
        || h.contains("typekit.net")
        || h.ends_with(".woff")
        || h.ends_with(".woff2")
        || h.contains(".woff2?")
}

fn inline_bytes(doc: &Html) -> InlineBundleBytes {
    let style_sel = Selector::parse("style").unwrap();
    let script_sel = Selector::parse("script:not([src])").unwrap();
    let css: usize = doc
        .select(&style_sel)
        .map(|el| el.text().map(str::len).sum::<usize>())
        .sum();
    let js: usize = doc
        .select(&script_sel)
        .map(|el| el.text().map(str::len).sum::<usize>())
        .sum();
    InlineBundleBytes {
        inline_css: css,
        inline_js: js,
    }
}

pub fn suggestions(p: &Performance) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    if p.images.eligible_for_lazy_missing > 0 {
        out.push(format!(
            "{} below-the-fold image{} without `loading=\"lazy\"`. Lazy-load every image past the first two to cut initial bytes.",
            p.images.eligible_for_lazy_missing,
            if p.images.eligible_for_lazy_missing == 1 { "" } else { "s" },
        ));
    }
    if p.images.missing_dimensions > 0 && p.images.total > 0 {
        // Codex 2026-05-24: "guaranteed CLS" was too strong — CSS
        // `aspect-ratio` or fixed-size containers can reserve space
        // without explicit width/height attrs.
        out.push(format!(
            "{} of {} `<img>` missing width/height. CLS risk unless reserved via CSS `aspect-ratio` / fixed-size container.",
            p.images.missing_dimensions, p.images.total,
        ));
    }
    if p.images.non_descriptive_filenames > 0 {
        out.push(format!(
            "{} image filename{} look templated (IMG_1234, DSC00012, screenshot.png). Rename — image search and multimodal AI read the filename.",
            p.images.non_descriptive_filenames,
            if p.images.non_descriptive_filenames == 1 { "" } else { "s" },
        ));
    }
    if p.images.total >= 4
        && p.images.modern_format_via_picture == 0
        && p.images.modern_format_via_src == 0
    {
        out.push(
            "No WebP / AVIF / JXL detected. Modern formats commonly cut ~25–50% versus comparable JPEG/PNG (content + encoder dependent; see developers.google.com/speed/webp, web.dev/articles/compress-images-avif).".into(),
        );
    }
    if p.render_blocking.head_scripts_blocking > 0 {
        out.push(format!(
            "{} render-blocking `<script>` in `<head>` (no async/defer/type=module). Each one delays first paint.",
            p.render_blocking.head_scripts_blocking,
        ));
    }
    if p.render_blocking.head_stylesheets > 4 {
        // Codex 2026-05-24: count alone is an arbitrary threshold; the
        // real cost rises with bytes too. Phrased as "severity rises
        // with count and bytes" — we count, we can't yet measure bytes.
        out.push(format!(
            "{} render-blocking `<link rel=\"stylesheet\">` in `<head>` (severity rises with count and bytes). Inline critical CSS, ship the rest async.",
            p.render_blocking.head_stylesheets,
        ));
    }
    if p.fonts.external_link_count > 0 && !p.fonts.has_font_display_strategy {
        out.push(
            "External fonts present but no `font-display: swap|optional|fallback` declared. Invisible text during font load (FOIT) on slow networks.".into(),
        );
    }
    if p.images.total >= 1 && !p.resource_hints.preloads_an_image && p.images.total > 0 {
        out.push(
            "No `<link rel=\"preload\" as=\"image\">` for the LCP image. Browser discovers the hero image only after the HTML parser reaches it.".into(),
        );
    }
    if p.inline_bytes.inline_css > 50_000 {
        // Codex 2026-05-24: 50KB raw is a noisy proxy; web.dev's
        // critical-CSS target is ~14KB compressed for the first round
        // trip. We see raw bytes, so phrase the threshold qualitatively.
        out.push(format!(
            "{} KB of inline `<style>` (raw). Keep critical CSS small — roughly under the first-round-trip budget when compressed (web.dev/extract-critical-css).",
            p.inline_bytes.inline_css / 1024,
        ));
    }
    if p.inline_bytes.inline_js > 50_000 {
        out.push(format!(
            "{} KB of inline `<script>`. Bundle and defer — inline JS blocks parsing.",
            p.inline_bytes.inline_js / 1024,
        ));
    }
    out
}
