//! Verify agent-info manifest matches reality.

use assert_cmd::Command;
use std::io::Write;

fn bin() -> Command {
    Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap()
}

fn agent_info() -> serde_json::Value {
    let out = bin().arg("agent-info").output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).expect("agent-info must be valid JSON")
}

fn fixture() -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".html")
        .tempfile()
        .expect("tempfile");
    writeln!(
        f,
        r#"<!DOCTYPE html><html><head><title>x</title></head><body><h1>x</h1></body></html>"#
    )
    .unwrap();
    f
}

// ── Required top-level fields ──────────────────────────────────────────────

#[test]
fn has_required_fields() {
    let info = agent_info();
    assert!(info["name"].is_string());
    assert!(info["version"].is_string());
    assert!(info["description"].is_string());
    assert!(info["commands"].is_object());
    assert!(info["exit_codes"].is_object());
    assert!(info["envelope"].is_object());
    assert!(info["auto_json_when_piped"].is_boolean());
}

#[test]
fn name_matches_binary() {
    let info = agent_info();
    assert_eq!(info["name"].as_str().unwrap(), env!("CARGO_PKG_NAME"));
}

#[test]
fn exit_codes_cover_full_contract() {
    let info = agent_info();
    let codes = &info["exit_codes"];
    for code in ["0", "1", "2", "3", "4"] {
        assert!(codes[code].is_string(), "exit_codes must document code {code}");
    }
}

#[test]
fn audit_is_routable() {
    let f = fixture();
    bin()
        .args(["audit", f.path().to_str().unwrap()])
        .assert()
        .code(0);
}

#[test]
fn agent_info_is_routable() {
    bin().arg("agent-info").assert().code(0);
}

#[test]
fn agent_info_alias_is_routable() {
    bin().arg("info").assert().code(0);
}

#[test]
fn skill_install_is_routable() {
    let tmp = tempfile::tempdir().unwrap();
    bin()
        .env("HOME", tmp.path())
        .args(["skill", "install"])
        .assert()
        .code(0);
}

#[test]
fn skill_status_is_routable() {
    bin().args(["skill", "status"]).assert().code(0);
}

#[test]
fn config_show_is_routable() {
    bin().args(["config", "show"]).assert().code(0);
}

#[test]
fn config_path_is_routable() {
    bin().args(["config", "path"]).assert().code(0);
}

#[test]
fn audit_has_arg_schema() {
    let info = agent_info();
    let audit = &info["commands"]["audit"];
    assert!(audit["args"].is_array(), "audit must have args array");

    let args = audit["args"].as_array().unwrap();
    assert!(!args.is_empty(), "audit must have at least one arg");
    assert_eq!(args[0]["name"], "file");
    assert_eq!(args[0]["required"], true);
}

#[test]
fn global_flags_documented() {
    let info = agent_info();
    let flags = &info["global_flags"];
    assert!(flags["--json"].is_object());
    assert!(flags["--quiet"].is_object());
}

#[test]
fn config_metadata_present() {
    let info = agent_info();
    let config = &info["config"];
    assert!(config["path"].is_string());
    assert!(config["env_prefix"].is_string());
    assert!(config["env_prefix"].as_str().unwrap().ends_with('_'));
}
