//! Checks the shell names against the reference's `getAvailableShellNames` in the vendored
//! `rule_shell_name.go`.

mod go_source;

use go_source::available_shell_names;
use workflint_data::{ShellPlatform, shell_names};

#[test]
fn shell_names_match_the_reference_function() {
    let ours = [
        ("platformKindAny", ShellPlatform::Any),
        ("platformKindWindows", ShellPlatform::Windows),
        ("platformKindMacOrLinux", ShellPlatform::MacOrLinux),
    ]
    .map(|(go, platform)| {
        let names = shell_names(platform)
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        (go.to_owned(), names)
    })
    .to_vec();
    assert_eq!(ours, available_shell_names());
}
