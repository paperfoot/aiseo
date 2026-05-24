//! Generate valid JSON-LD blocks for the five most-used schema types.
//!
//! Each variant is intentionally minimal — only the fields that earn
//! their place in rich results / AI-citation surfaces. Schema.org allows
//! ~50+ fields on Article alone; we ship the ones agents actually need.

use serde::Serialize;
use serde_json::json;

use crate::error::AppError;
use crate::output::{self, Ctx};

#[derive(clap::Subcommand)]
pub enum SchemaType {
    /// FAQPage with one or more Question/Answer pairs. Highest-impact
    /// AI-citation signal of the bunch.
    Faq {
        /// Question/answer pairs as `question::answer`. Repeat for more.
        #[arg(long = "qa", value_name = "QUESTION::ANSWER", required = true)]
        qa: Vec<String>,
    },
    /// Article schema. Use for blog posts, long-form pages, news.
    Article {
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: String,
        /// ISO 8601 publication date (e.g. 2026-05-24)
        #[arg(long = "date-published")]
        date_published: String,
        /// ISO 8601 last-modified date; defaults to date-published
        #[arg(long = "date-modified")]
        date_modified: Option<String>,
        #[arg(long)]
        author: String,
        /// Author credentials suffix (MD, PhD, etc.)
        #[arg(long)]
        credentials: Option<String>,
        #[arg(long = "author-url")]
        author_url: Option<String>,
        #[arg(long = "org-name")]
        org_name: Option<String>,
        #[arg(long = "org-url")]
        org_url: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        image: Option<String>,
    },
    /// HowTo schema. Steps repeat.
    Howto {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        /// Step text; pass once per step.
        #[arg(long, required = true)]
        step: Vec<String>,
    },
    /// Organization schema.
    Organization {
        #[arg(long)]
        name: String,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        logo: Option<String>,
        #[arg(long = "same-as", value_name = "URL")]
        same_as: Vec<String>,
    },
    /// Person schema. Useful for author bylines.
    Person {
        #[arg(long)]
        name: String,
        #[arg(long = "job-title")]
        job_title: Option<String>,
        /// Credential suffix (MD, PhD, etc.)
        #[arg(long)]
        credentials: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long = "works-for")]
        works_for: Option<String>,
    },
}

#[derive(Serialize)]
struct SchemaEnvelope {
    schema_type: &'static str,
    json_ld: serde_json::Value,
}

pub fn run(ctx: Ctx, kind: SchemaType) -> Result<(), AppError> {
    let (schema_type, json_ld) = match kind {
        SchemaType::Faq { qa } => ("FAQPage", build_faq(qa)?),
        SchemaType::Article {
            title,
            description,
            date_published,
            date_modified,
            author,
            credentials,
            author_url,
            org_name,
            org_url,
            url,
            image,
        } => (
            "Article",
            build_article(
                title,
                description,
                date_published,
                date_modified,
                author,
                credentials,
                author_url,
                org_name,
                org_url,
                url,
                image,
            ),
        ),
        SchemaType::Howto {
            name,
            description,
            step,
        } => ("HowTo", build_howto(name, description, step)),
        SchemaType::Organization {
            name,
            url,
            logo,
            same_as,
        } => ("Organization", build_organization(name, url, logo, same_as)),
        SchemaType::Person {
            name,
            job_title,
            credentials,
            url,
            works_for,
        } => (
            "Person",
            build_person(name, job_title, credentials, url, works_for),
        ),
    };

    let envelope = SchemaEnvelope {
        schema_type,
        json_ld,
    };

    output::print_success_or(ctx, &envelope, |e| {
        // Human view: just print the JSON-LD ready to paste into <script>.
        println!("<script type=\"application/ld+json\">");
        println!("{}", serde_json::to_string_pretty(&e.json_ld).unwrap());
        println!("</script>");
    });

    Ok(())
}

fn build_faq(qa: Vec<String>) -> Result<serde_json::Value, AppError> {
    let mut entities = Vec::new();
    for pair in qa {
        let (q, a) = pair.split_once("::").ok_or_else(|| {
            AppError::InvalidInput(format!(
                "expected `question::answer`, got `{pair}`"
            ))
        })?;
        entities.push(json!({
            "@type": "Question",
            "name": q.trim(),
            "acceptedAnswer": {
                "@type": "Answer",
                "text": a.trim()
            }
        }));
    }
    Ok(json!({
        "@context": "https://schema.org",
        "@type": "FAQPage",
        "mainEntity": entities
    }))
}

fn build_article(
    title: String,
    description: String,
    date_published: String,
    date_modified: Option<String>,
    author: String,
    credentials: Option<String>,
    author_url: Option<String>,
    org_name: Option<String>,
    org_url: Option<String>,
    url: Option<String>,
    image: Option<String>,
) -> serde_json::Value {
    let date_modified = date_modified.unwrap_or_else(|| date_published.clone());

    let mut author_obj = json!({
        "@type": "Person",
        "name": author,
    });
    if let Some(c) = credentials {
        author_obj["honorificSuffix"] = json!(c);
    }
    if let Some(u) = author_url {
        author_obj["url"] = json!(u);
    }

    let mut out = json!({
        "@context": "https://schema.org",
        "@type": "Article",
        "headline": title,
        "description": description,
        "datePublished": date_published,
        "dateModified": date_modified,
        "author": author_obj,
    });

    if let Some(name) = org_name {
        let mut org = json!({ "@type": "Organization", "name": name });
        if let Some(u) = org_url {
            org["url"] = json!(u);
        }
        out["publisher"] = org;
    }
    if let Some(u) = url {
        out["mainEntityOfPage"] = json!({ "@type": "WebPage", "@id": u });
    }
    if let Some(i) = image {
        out["image"] = json!(i);
    }
    out
}

fn build_howto(name: String, description: Option<String>, steps: Vec<String>) -> serde_json::Value {
    let steps_json: Vec<serde_json::Value> = steps
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "@type": "HowToStep",
                "position": i + 1,
                "text": s
            })
        })
        .collect();
    let mut out = json!({
        "@context": "https://schema.org",
        "@type": "HowTo",
        "name": name,
        "step": steps_json,
    });
    if let Some(d) = description {
        out["description"] = json!(d);
    }
    out
}

fn build_organization(
    name: String,
    url: Option<String>,
    logo: Option<String>,
    same_as: Vec<String>,
) -> serde_json::Value {
    let mut out = json!({
        "@context": "https://schema.org",
        "@type": "Organization",
        "name": name,
    });
    if let Some(u) = url {
        out["url"] = json!(u);
    }
    if let Some(l) = logo {
        out["logo"] = json!(l);
    }
    if !same_as.is_empty() {
        out["sameAs"] = json!(same_as);
    }
    out
}

fn build_person(
    name: String,
    job_title: Option<String>,
    credentials: Option<String>,
    url: Option<String>,
    works_for: Option<String>,
) -> serde_json::Value {
    let mut out = json!({
        "@context": "https://schema.org",
        "@type": "Person",
        "name": name,
    });
    if let Some(t) = job_title {
        out["jobTitle"] = json!(t);
    }
    if let Some(c) = credentials {
        out["honorificSuffix"] = json!(c);
    }
    if let Some(u) = url {
        out["url"] = json!(u);
    }
    if let Some(w) = works_for {
        out["worksFor"] = json!({ "@type": "Organization", "name": w });
    }
    out
}
