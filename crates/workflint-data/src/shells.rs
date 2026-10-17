//! The shell names that `shell:` accepts on each platform.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `getAvailableShellNames` in
//! `rule_shell_name.go`, lines 117-147, which follows
//! <https://docs.github.com/en/actions/learn-github-actions/workflow-syntax-for-github-actions#using-a-specific-shell>.
//! Until M7 replaces it with a generated file (ADR 0011), keep it equal to that function. The
//! test `shell_names_match_the_reference_function` compares them.

/// The platforms that a job's `runs-on` can name, as far as shells are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellPlatform {
    /// The platform is not known, so every shell of any platform is accepted.
    Any,
    /// Windows.
    Windows,
    /// macOS or Linux.
    MacOrLinux,
}

/// Returns the shell names that `platform` accepts, in the reference's order. A `shell:` value
/// that is not a name, such as `perl {0}`, is a custom shell.
#[must_use]
pub const fn shell_names(platform: ShellPlatform) -> &'static [&'static str] {
    match platform {
        ShellPlatform::Any => &["bash", "pwsh", "python", "sh", "cmd", "powershell"],
        ShellPlatform::Windows => &["bash", "pwsh", "python", "cmd", "powershell"],
        ShellPlatform::MacOrLinux => &["bash", "pwsh", "python", "sh"],
    }
}
