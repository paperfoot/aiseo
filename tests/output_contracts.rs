//! Verify JSON envelope structure on stdout/stderr.
//!
//! assert_cmd runs the binary in a pipe (not a TTY), so JSON auto-detection
//! kicks in — no need for `--json`.

use assert_cmd::Command;
use std::io::Write;

fn bin() -> Command {
    Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap()
}

fn fixture() -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".html")
        .tempfile()
        .expect("tempfile");
    writeln!(
        f,
        r#"<!DOCTYPE html><html><head><title>Test page about cholesterol</title><meta name="description" content="A May 2026 guide to optimal LDL cholesterol targets and the evidence behind aggressive lipid lowering with statins and PCSK9 inhibitors."><meta property="og:title" content="Test"><meta property="og:image" content="https://example.com/og.png"></head><body><h1>LDL Cholesterol</h1><p><strong>TL;DR</strong>: Aim for LDL under 70 mg/dL.</p><h2>Why</h2><p>30% relative risk reduction with PCSK9 inhibitors.</p><h2>How</h2><p>Statins first-line.</p></body></html>"#
    )
    .unwrap();
    f
}

// ── Success envelope ───────────────────────────────────────────────────────

#[test]
fn success_envelope_shape() {
    let f = fixture();
    let out = bin().args(["audit", f.path().to_str().unwrap()]).output().unwrap();

    assert!(out.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout should be valid JSON");

    assert_eq!(json["version"], "1");
    assert_eq!(json["status"], "success");
    assert!(json["data"].is_object(), "envelope must have a 'data' field");
    assert!(json["data"]["score"].is_number(), "audit must report a score");
    assert!(json["data"]["suggestions"].is_array(), "audit must report suggestions");
}

#[test]
fn success_envelope_with_json_flag() {
    let f = fixture();
    let out = bin()
        .args(["audit", f.path().to_str().unwrap(), "--json"])
        .output()
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--json stdout should be valid JSON");

    assert_eq!(json["status"], "success");
    assert_eq!(json["data"]["file_type"], "html");
}

// ── Error envelope ─────────────────────────────────────────────────────────

#[test]
fn error_envelope_shape() {
    let out = bin().args(["contract", "3"]).output().unwrap();

    assert!(!out.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&out.stderr).expect("stderr should be valid JSON");

    assert_eq!(json["version"], "1");
    assert_eq!(json["status"], "error");
    assert!(
        json["error"].is_object(),
        "error envelope must have 'error' field"
    );
    assert!(json["error"]["code"].is_string(), "error must have 'code'");
    assert!(
        json["error"]["message"].is_string(),
        "error must have 'message'"
    );
    assert!(
        json["error"]["suggestion"].is_string(),
        "error must have 'suggestion'"
    );
}

#[test]
fn error_code_matches_variant() {
    let out = bin().args(["contract", "2"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(json["error"]["code"], "config_error");

    let out = bin().args(["contract", "4"]).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(json["error"]["code"], "rate_limited");
}

// ── Help/version wrapping ──────────────────────────────────────────────────

#[test]
fn help_wrapped_in_envelope_when_piped() {
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("piped --help should be JSON");

    assert_eq!(json["status"], "success");
    assert!(json["data"]["usage"].is_string());
}

#[test]
fn version_wrapped_in_envelope_when_piped() {
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("piped --version should be JSON");

    assert_eq!(json["status"], "success");
}

// ── Quiet flag ─────────────────────────────────────────────────────────────

#[test]
fn quiet_still_emits_json() {
    let f = fixture();
    let out = bin()
        .args(["audit", f.path().to_str().unwrap(), "--json", "--quiet"])
        .output()
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--quiet should not suppress JSON");

    assert_eq!(json["status"], "success");
}

// ── Parse error envelope ───────────────────────────────────────────────────

#[test]
fn parse_error_wrapped_in_envelope() {
    let out = bin()
        .arg("audit") // missing required <file>
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(3));

    let json: serde_json::Value =
        serde_json::from_slice(&out.stderr).expect("parse error should be JSON on stderr");

    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "invalid_input");
    assert!(
        json["error"]["suggestion"]
            .as_str()
            .unwrap()
            .contains("--help")
    );
}
