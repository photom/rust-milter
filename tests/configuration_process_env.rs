//! Environment variable scenarios that need the process environment
//! (CFG-094 and CFG-095) from `docs/testing/configuration.md`: only it can
//! hold text that is not valid UTF-8.
//!
//! Changing the environment of a running test needs `unsafe`, which the crate
//! forbids. Each test instead runs this test binary again with the variable
//! set, and the child process, which sees the variable, checks the result.
#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::Command;

use rust_milter::{ConfigError, ServerConfig};

/// Runs `check` in a child process whose environment has `name` = `value`.
/// In the child, where the variable is set, it runs `check` directly.
fn with_var(test: &str, name: &OsStr, value: &OsStr, check: impl FnOnce()) {
    if std::env::var_os(name).is_some() {
        check();
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--test-threads=1"])
        .env(name, value)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the child process failed:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cfg_094_rejects_a_name_with_the_prefix_that_is_not_utf8() {
    let name = OsStr::from_bytes(b"CFG094_\xFF");

    with_var(
        "cfg_094_rejects_a_name_with_the_prefix_that_is_not_utf8",
        name,
        OsStr::new("1"),
        || {
            assert_eq!(
                ServerConfig::from_env("CFG094_"),
                Err(ConfigError::UnknownEnvVariable(
                    "CFG094_\u{FFFD}".to_owned()
                ))
            );
        },
    );
}

#[test]
fn cfg_095_rejects_a_value_that_is_not_utf8() {
    let value = OsStr::from_bytes(b"10\xFF");

    with_var(
        "cfg_095_rejects_a_value_that_is_not_utf8",
        OsStr::new("CFG095_BACKLOG"),
        value,
        || {
            let result = ServerConfig::from_env("CFG095_");
            assert!(
                matches!(&result, Err(ConfigError::InvalidEnvValue { variable, .. }) if variable == "CFG095_BACKLOG"),
                "got {result:?}"
            );
        },
    );
}
