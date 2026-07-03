//! Verify the semantic exit-code contract (0-4).
//!
//! Uses the hidden `contract` command for deterministic triggers and
//! real commands for natural exit-code coverage.

use assert_cmd::Command;
use std::io::Write;

mod common;
use common::aiseo_in;

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

// ── Contract command: deterministic 0-4 ────────────────────────────────────

#[test]
fn contract_exit_0() {
    bin().args(["contract", "0"]).assert().code(0);
}

#[test]
fn contract_exit_1_transient() {
    bin().args(["contract", "1"]).assert().code(1);
}

#[test]
fn contract_exit_2_config() {
    bin().args(["contract", "2"]).assert().code(2);
}

#[test]
fn contract_exit_3_bad_input() {
    bin().args(["contract", "3"]).assert().code(3);
}

#[test]
fn contract_exit_4_rate_limited() {
    bin().args(["contract", "4"]).assert().code(4);
}

// ── Real commands: natural exit codes ──────────────────────────────────────

#[test]
fn audit_success_exits_0() {
    let f = fixture();
    bin().args(["audit", f.path().to_str().unwrap()]).assert().code(0);
}

#[test]
fn help_exits_0() {
    bin().arg("--help").assert().code(0);
}

#[test]
fn version_exits_0() {
    bin().arg("--version").assert().code(0);
}

#[test]
fn agent_info_exits_0() {
    bin().arg("agent-info").assert().code(0);
}

#[test]
fn config_path_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    aiseo_in(tmp.path()).args(["config", "path"]).assert().code(0);
}

#[test]
fn config_show_exits_0() {
    let tmp = tempfile::tempdir().unwrap();
    aiseo_in(tmp.path()).args(["config", "show"]).assert().code(0);
}

#[test]
fn missing_subcommand_exits_3() {
    // No subcommand at all is a parse error.
    bin().assert().code(3);
}

#[test]
fn audit_missing_file_exits_3() {
    // `audit` requires a positional <file>.
    bin().arg("audit").assert().code(3);
}

#[test]
fn audit_nonexistent_file_exits_3() {
    bin()
        .args(["audit", "/tmp/aiseo-does-not-exist.html"])
        .assert()
        .code(3);
}
