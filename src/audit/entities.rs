//! People and organisation extraction.
//!
//! Heuristic — picks up named persons with credentials (the Princeton GEO
//! signal) and organisation names with common legal suffixes or
//! capitalisation patterns. Not a NER model; intentionally crude so an
//! agent can decide whether to trust the result.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub struct Entities {
    pub people: Vec<Person>,
    pub organizations: Vec<String>,
}

#[derive(Serialize)]
pub struct Person {
    pub name: String,
    /// Credential suffix like "MD", "PhD", "PharmD" if found within ~8
    /// tokens of the name. Absent if no credential is nearby.
    pub credentials: Option<String>,
}

// "Dr Jane Smith", "Dr. Jane Smith", "Professor Alice Brown"
static TITLED_NAME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:Dr\.?|Prof\.?|Professor|Mr|Mrs|Ms|Sir|Dame)\s+([A-Z][a-zA-Z\-']+(?:\s+[A-Z][a-zA-Z\-']+){1,3})")
        .unwrap()
});

// "Jane Smith, MD" / "Alice Brown, PhD" — credentials trailing the name.
static NAME_WITH_CRED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b([A-Z][a-zA-Z\-']+(?:\s+[A-Z][a-zA-Z\-']+){1,3})\s*,?\s*(MD|Ph\.?D\.?|MBA|MSc|MPH|DDS|DMD|JD|RN|DO|DPM|OD|PharmD|DVM|EdD|PsyD)\b",
    )
    .unwrap()
});

// Organisations: legal suffix anchored
static ORG_LEGAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b([A-Z][A-Za-z0-9&\-]+(?:\s+[A-Z][A-Za-z0-9&\-]+){0,4})\s+(Inc\.?|LLC|Ltd\.?|Limited|Co\.|Corp\.?|Corporation|Foundation|Institute|University|College|Society|Group|Holdings|Hospital|Clinic|Laboratory|Labs|Research)\b",
    )
    .unwrap()
});

/// Honorifics / sentence-starters that should never appear at the start of
/// a person's name. The credential-suffix regex (intentionally loose) can
/// capture them otherwise.
const HONORIFIC_PREFIXES: &[&str] =
    &["Per", "By", "With", "From", "As", "Like", "And", "The", "Dr", "Prof", "Mr", "Mrs", "Ms"];

pub fn extract(body_text: &str) -> Entities {
    let mut people: Vec<Person> = Vec::new();
    let mut seen_names: BTreeSet<String> = BTreeSet::new();

    for cap in NAME_WITH_CRED.captures_iter(body_text) {
        let name = strip_honorific(&cap[1]);
        let cred = cap[2].to_string();
        if name.split_whitespace().count() < 2 {
            continue; // single-word "names" are almost always false positives
        }
        if seen_names.insert(name.clone()) {
            people.push(Person {
                name,
                credentials: Some(normalise_cred(&cred)),
            });
        }
    }

    for cap in TITLED_NAME.captures_iter(body_text) {
        let name = strip_honorific(&cap[1]);
        if name.split_whitespace().count() < 2 {
            continue;
        }
        if seen_names.insert(name.clone()) {
            people.push(Person {
                name,
                credentials: None,
            });
        }
    }

    // Organisations: only the high-confidence legal-suffix matches. Bare
    // acronyms (LDL, MRI, FDA) produce more false positives than signal
    // for SEO content, so we leave them to the keyword pass.
    let mut orgs: BTreeSet<String> = BTreeSet::new();
    for cap in ORG_LEGAL.captures_iter(body_text) {
        orgs.insert(format!("{} {}", &cap[1], &cap[2]));
    }

    let mut organizations: Vec<String> = orgs.into_iter().collect();
    organizations.sort();
    organizations.truncate(20);

    Entities {
        people,
        organizations,
    }
}

fn strip_honorific(name: &str) -> String {
    let mut tokens: Vec<&str> = name.split_whitespace().collect();
    while let Some(first) = tokens.first() {
        if HONORIFIC_PREFIXES.iter().any(|h| h.eq_ignore_ascii_case(first)) {
            tokens.remove(0);
        } else {
            break;
        }
    }
    tokens.join(" ")
}

fn normalise_cred(c: &str) -> String {
    c.replace('.', "").to_uppercase()
}
