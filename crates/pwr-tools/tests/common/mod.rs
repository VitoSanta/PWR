//! Shared by the integration tests that depend on what the host has.
//!
//! A test that returns early because Docker, .NET or a browser is missing
//! counts as a pass, and a run of them said nothing about what it had not
//! exercised. `skip` makes the omission a line of output.

use std::io::Write as _;

/// Records that the running test did not exercise what it names, and why.
///
/// Prints `PWR-SKIP <test> <reason>` and, when `PWR_SKIP_LOG` names a file,
/// appends the same line to it. The file is what CI reads: cargo captures the
/// output of a passing test, so a line printed here would never reach the log.
#[allow(dead_code)]
pub fn skip(reason: &str) {
    let log = std::env::var_os("PWR_SKIP_LOG").map(std::path::PathBuf::from);
    skip_to(log.as_deref(), reason);
}

/// [`skip`] with the log named by the caller.
pub fn skip_to(log: Option<&std::path::Path>, reason: &str) {
    let test = std::thread::current()
        .name()
        .unwrap_or("unnamed test")
        .to_owned();
    let line = format!("PWR-SKIP {test} {reason}");
    eprintln!("{line}");
    if let Some(path) = log
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}
