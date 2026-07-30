//! End-to-end tests against the built binary.
//!
//! These are the tests that catch what unit tests structurally cannot: exit
//! codes, what lands on stdout vs stderr, and whether a secret leaked into
//! output.
//!
//! Integration tests in `tests/` are compiled as their own crate *without*
//! `cfg(test)` set, so clippy.toml's `allow-unwrap-in-tests` does not reach
//! them. The allow has to be stated here instead.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt as _;
use predicates::str::contains;

fn bin() -> Command {
    Command::cargo_bin(env!("CARGO_PKG_NAME")).expect("binary should build")
}

/// Isolate from the developer's own config files and environment, which would
/// otherwise make these pass or fail depending on whose machine they run on.
fn isolated(sandbox: &pc_testkit::Sandbox) -> Command {
    let mut cmd = bin();
    cmd.current_dir(sandbox.root());
    cmd.env_clear();
    cmd
}

#[test]
fn print_config_writes_to_stdout_and_succeeds() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    isolated(&sandbox)
        .arg("print-config")
        .assert()
        .success()
        .stdout(contains("\"workers\""));
}

#[test]
fn print_config_redacts_the_api_key() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    sandbox
        .write("config.toml", "api_key = \"super-secret-value\"\n")
        .unwrap();

    isolated(&sandbox)
        .arg("print-config")
        .assert()
        .success()
        .stdout(contains("super-secret-value").not())
        .stdout(contains("REDACTED"));
}

#[test]
fn a_config_file_in_the_working_directory_is_picked_up() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    sandbox.write("config.toml", "workers = 11\n").unwrap();

    isolated(&sandbox)
        .arg("print-config")
        .assert()
        .success()
        .stdout(contains("\"workers\": 11"));
}

#[test]
fn a_cli_flag_beats_the_config_file() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    sandbox.write("config.toml", "workers = 11\n").unwrap();

    isolated(&sandbox)
        .args(["print-config", "--workers", "3"])
        .assert()
        .success()
        .stdout(contains("\"workers\": 3"));
}

#[test]
fn an_environment_variable_beats_the_config_file() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    sandbox.write("config.toml", "workers = 11\n").unwrap();

    isolated(&sandbox)
        .env("{{env_prefix}}_WORKERS", "7")
        .arg("print-config")
        .assert()
        .success()
        .stdout(contains("\"workers\": 7"));
}

#[test]
fn a_missing_api_key_exits_64_and_explains_itself() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    isolated(&sandbox)
        .arg("run")
        .assert()
        .failure()
        // 64 = EX_USAGE, from Code::Invalid. A caller can branch on this
        // without parsing our text.
        .code(64)
        .stderr(contains("api_key is not set"));
}

#[test]
fn a_named_but_missing_config_file_exits_66() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    isolated(&sandbox)
        .args(["--config", "does-not-exist.toml", "run"])
        .assert()
        .failure()
        // 66 = EX_NOINPUT, from Code::NotFound.
        .code(66);
}

#[test]
fn a_valid_run_succeeds_and_logs_nothing_to_stdout() {
    let sandbox = pc_testkit::Sandbox::new().unwrap();
    sandbox
        .write("config.toml", "api_key = \"present\"\n")
        .unwrap();

    isolated(&sandbox)
        .arg("run")
        .assert()
        .success()
        // Logs belong on stderr so that stdout stays machine-readable.
        .stdout("");
}
