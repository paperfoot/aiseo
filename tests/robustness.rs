//! Robustness tests: verify recovery from bad state.
//!
//! These tests ensure discovery and diagnostic commands work even when
//! configuration is malformed, and that enforced constraints match agent-info.

use assert_cmd::Command;
use std::io::Write;

mod common;
use common::{aiseo_in, write_config_in};

fn bin() -> Command {
    Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap()
}

fn fixture() -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(".html")
        .tempfile()
        .unwrap();
    writeln!(f, "<html><body><h1>x</h1></body></html>").unwrap();
    f
}

// ── Malformed config resilience ────────────────────────────────────────────

/// agent-info must work even with a broken config file.
#[test]
fn agent_info_works_with_malformed_config() {
    let tmp = tempfile::tempdir().unwrap();
    write_config_in(tmp.path(), "{{invalid toml");

    aiseo_in(tmp.path()).arg("agent-info").assert().code(0);
}

/// config path must work even with a broken config file.
#[test]
fn config_path_works_with_malformed_config() {
    let tmp = tempfile::tempdir().unwrap();
    write_config_in(tmp.path(), "{{invalid toml");

    aiseo_in(tmp.path()).args(["config", "path"]).assert().code(0);
}

/// config show should fail gracefully with exit 2 on malformed config.
#[test]
fn config_show_fails_with_malformed_config() {
    let tmp = tempfile::tempdir().unwrap();
    write_config_in(tmp.path(), "{{invalid toml");

    aiseo_in(tmp.path()).args(["config", "show"]).assert().code(2);
}

// ── Constraint enforcement ─────────────────────────────────────────────────

/// Unknown flag must be rejected by clap (exit 3), not silently absorbed.
#[test]
fn unknown_flag_rejected() {
    let f = fixture();
    bin()
        .args(["audit", f.path().to_str().unwrap(), "--nonsense"])
        .assert()
        .code(3);
}

/// audit command works even when HOME is unusual.
#[test]
fn audit_works_with_temp_home() {
    let tmp = tempfile::tempdir().unwrap();
    let f = fixture();
    aiseo_in(tmp.path())
        .args(["audit", f.path().to_str().unwrap()])
        .assert()
        .code(0);
}
