//! Link graph signals.
//!
//! Counts internal vs external links in main content (chrome already
//! filtered upstream by the body-text walker, but link extraction
//! operates on the full DOM and filters by ancestor).
//!
//! Outbound-authority proxy: ratio of links pointing at well-known
//! editorial / governmental / scholarly domains versus everything else.
//! Not perfect — every site has its own outbound mix — but the absence
//! of any authoritative outbound on a long page is a content-quality
//! finding for AEO surfaces that reward citation discipline.

use scraper::{Html, Selector};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub struct LinkGraph {
    /// Total `<a href>` in main content (excludes nav/footer/aside,
    /// banner-style headers).
    pub total: usize,
    /// Links whose href targets the same host the page declares as its
    /// canonical (or, when canonical is absent, are href-relative).
    pub internal: usize,
    /// Links pointing at a different host than canonical.
    pub external: usize,
    /// Subset of `external` with `rel="nofollow"`.
    pub nofollow_external: usize,
    /// External links pointing at editorial / governmental / academic
    /// authority hosts (heuristic; see AUTHORITY_HOSTS).
    pub authority_external: usize,
    /// Number of unique external hosts linked.
    pub external_hosts: usize,
    /// Anchors with empty or trivially generic anchor text
    /// ("click here", "read more", "link", "here") — these convey no
    /// topic signal and hurt accessibility.
    pub generic_anchor_text: usize,
}

/// Conservative "trusted reference" outbound-host list — NOT a hard GEO
/// ranking signal. Pages that cite outside this list still get the rest
/// of the audit. The set distinguishes:
///   - Governmental + intergovernmental (primary policy / regulatory)
///   - .edu + academic TLDs (institutional authority)
///   - Peer-reviewed journals (primary evidence)
///   - Preprint / repository (signal, not peer-reviewed authority)
///   - Editorial reference / major press (corroborating)
///
/// Codex 2026-05-24 expanded the set; previous "authority" framing
/// over-claimed — Google's helpful-content guidance treats clear
/// sourcing as a trust signal, not a documented ranking lever.
const AUTHORITY_DOMAINS: &[&str] = &[
    // Governmental + intergovernmental
    "nih.gov",
    "cdc.gov",
    "fda.gov",
    "who.int",
    "europa.eu",
    "ema.europa.eu",
    "nice.org.uk",
    "gov.uk",
    "nhs.uk",
    "ac.uk",
    "oecd.org",
    "worldbank.org",
    "imf.org",
    "un.org",
    "unesco.org",
    // Institutional / academic
    ".edu",
    ".ac.uk",
    ".ac.jp",
    "ox.ac.uk",
    "cam.ac.uk",
    "harvard.edu",
    "mit.edu",
    "stanford.edu",
    // Peer-reviewed journals
    "doi.org",
    "pubmed.ncbi.nlm.nih.gov",
    "ncbi.nlm.nih.gov",
    "nature.com",
    "science.org",
    "cell.com",
    "nejm.org",
    "thelancet.com",
    "bmj.com",
    "jamanetwork.com",
    "pnas.org",
    "plos.org",
    "frontiersin.org",
    "springer.com",
    "wiley.com",
    "sciencedirect.com",
    "cochranelibrary.com",
    // Preprint / repository (signal, not peer-reviewed authority)
    "arxiv.org",
    "biorxiv.org",
    "medrxiv.org",
    "ssrn.com",
    "scholar.google",
    // Editorial reference / major press
    "wikipedia.org",
    "britannica.com",
    "reuters.com",
    "apnews.com",
    "bloomberg.com",
    "ft.com",
    "wsj.com",
    "nytimes.com",
    "economist.com",
    "bbc.com",
    "bbc.co.uk",
    "theguardian.com",
];

const GENERIC_ANCHOR: &[&str] =
    &["click here", "read more", "link", "here", "more", "this", "this link", "details"];

pub fn extract(doc: &Html, canonical: Option<&str>) -> LinkGraph {
    let canonical_host = canonical.and_then(host_of);

    let sel = Selector::parse("a[href]").unwrap();

    let mut total = 0;
    let mut internal = 0;
    let mut external = 0;
    let mut nofollow_external = 0;
    let mut authority = 0;
    let mut external_hosts: BTreeSet<String> = BTreeSet::new();
    let mut generic = 0;

    for el in doc.select(&sel) {
        if is_in_chrome(el) {
            continue;
        }
        let href = el.value().attr("href").unwrap_or("").trim();
        if href.is_empty() || href.starts_with('#') || href.starts_with("javascript:") {
            continue;
        }
        total += 1;

        let text: String = el
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let text_lower = text.to_ascii_lowercase();
        if GENERIC_ANCHOR.contains(&text_lower.as_str()) || text.is_empty() {
            generic += 1;
        }

        let link_host = host_of(href);
        match (link_host.as_deref(), canonical_host.as_deref()) {
            (None, _) => internal += 1, // relative href → same site
            (Some(lh), Some(ch)) if hosts_equal(lh, ch) => internal += 1,
            (Some(lh), _) => {
                external += 1;
                external_hosts.insert(lh.to_string());
                let rel = el.value().attr("rel").unwrap_or("").to_ascii_lowercase();
                if rel.split_whitespace().any(|t| t == "nofollow") {
                    nofollow_external += 1;
                }
                if is_authority(lh) {
                    authority += 1;
                }
            }
        }
    }

    LinkGraph {
        total,
        internal,
        external,
        nofollow_external,
        authority_external: authority,
        external_hosts: external_hosts.len(),
        generic_anchor_text: generic,
    }
}

fn host_of(href: &str) -> Option<String> {
    // Crude but adequate: only treat URLs with a scheme as having an
    // explicit host. `/foo`, `?x=1`, `foo.html` are internal.
    let lower = href.trim().to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("//")) {
        return None;
    }
    let after_scheme = lower
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("//");
    let host = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn hosts_equal(a: &str, b: &str) -> bool {
    let a = a.trim_start_matches("www.");
    let b = b.trim_start_matches("www.");
    a.eq_ignore_ascii_case(b)
}

fn is_authority(host: &str) -> bool {
    let h = host.trim_start_matches("www.").to_ascii_lowercase();
    AUTHORITY_DOMAINS.iter().any(|d| {
        let d = d.trim_start_matches("www.");
        // TLD-style entries start with a dot ("edu", "ac.uk") — match
        // as a suffix on any host. Domain entries match either exactly
        // or as a parent domain.
        if let Some(suffix) = d.strip_prefix('.') {
            h.ends_with(suffix) && h.contains(&format!(".{suffix}"))
                || h.ends_with(&format!(".{suffix}"))
                || h.ends_with(suffix)
        } else {
            h == *d || h.ends_with(&format!(".{d}"))
        }
    })
}

fn is_in_chrome(el: scraper::ElementRef<'_>) -> bool {
    let mut node = el.parent();
    while let Some(n) = node {
        if let scraper::Node::Element(elem) = n.value() {
            match elem.name() {
                "nav" | "footer" | "aside" => return true,
                "header" => {
                    if let Some(eref) = scraper::ElementRef::wrap(n)
                        && header_has_nav(eref)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        node = n.parent();
    }
    false
}

fn header_has_nav(el: scraper::ElementRef<'_>) -> bool {
    let nav_sel = Selector::parse("nav").unwrap();
    el.select(&nav_sel).next().is_some()
}

pub fn suggestions(lg: &LinkGraph, word_count: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if word_count >= 600 && lg.authority_external == 0 {
        // Codex 2026-05-24: this is a trust / sourcing signal, not a
        // ranking factor. Google's helpful-content guidance treats clear
        // sourcing as one signal among many. Worded accordingly.
        out.push(
            "No outbound links to trusted-reference sources (.gov, .edu, peer-reviewed journals, major press) in main content. Clear sourcing is a trust signal for both readers and AI retrievers.".into(),
        );
    }
    if lg.generic_anchor_text >= 3 {
        out.push(format!(
            "{} generic anchor texts (\"click here\", \"read more\", \"link\"). Anchor text is topic signal — use descriptive phrases.",
            lg.generic_anchor_text,
        ));
    }
    if lg.total >= 30 && lg.external == 0 {
        out.push(
            "Many internal links and zero outbound. Pages that never cite outside read as link-mill / templated to AI retrievers.".into(),
        );
    }
    out
}
