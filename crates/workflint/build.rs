//! Records the version of the compiler for the third line of `workflint -version`.

use std::env;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-env-changed=RUSTC");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    // `rustc --version` prints `rustc 1.97.0 (<hash> <date>)`.
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo::rustc-env=WORKFLINT_RUSTC_VERSION={version}");
}
