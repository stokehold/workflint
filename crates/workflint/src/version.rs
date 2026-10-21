//! The output of `-version` and the version that the help links to (ADR 0014).

/// The version of workflint.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How the release pipeline installed this build, if it built it (M6.2).
const BUILT_INSTALLED_FROM: Option<&str> = option_env!("WORKFLINT_INSTALLED_FROM");

/// The second line of `-version` for a build that nothing else describes.
const FROM_SOURCE: &str = "installed by building from source";

/// The three lines of `-version`: the bare version, how workflint was installed, and the
/// compiler and target it was built with. `installed_from` is the value of the
/// `WORKFLINT_INSTALLED_FROM` environment variable, which a package's launcher sets.
pub(crate) fn lines(installed_from: Option<&str>) -> String {
    let installed_from = installed_from
        .filter(|value| !value.is_empty())
        .or(BUILT_INSTALLED_FROM)
        .unwrap_or(FROM_SOURCE);
    format!(
        "{VERSION}\n{installed_from}\nbuilt with rustc {} for {}/{}\n",
        env!("WORKFLINT_RUSTC_VERSION"),
        go_os(std::env::consts::OS),
        go_arch(std::env::consts::ARCH),
    )
}

/// The Git ref of the documentation that matches this build: the release tag for a build of
/// the release pipeline, and `main` for any other build.
pub(crate) fn docs_ref() -> String {
    let release = VERSION
        .split('.')
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if BUILT_INSTALLED_FROM.is_some() && release && VERSION.split('.').count() == 3 {
        format!("v{VERSION}")
    } else {
        "main".to_owned()
    }
}

/// Go's name of the operating system, `GOOS`, which the release archives use (ADR 0014).
fn go_os(os: &str) -> &str {
    match os {
        "macos" => "darwin",
        other => other,
    }
}

/// Go's name of the architecture, `GOARCH`, which the release archives use (ADR 0014).
fn go_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        "powerpc64" => "ppc64",
        "loongarch64" => "loong64",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_is_the_bare_version() {
        assert_eq!(lines(None).lines().next(), Some(VERSION));
    }

    #[test]
    fn the_environment_variable_names_the_install_channel() {
        assert_eq!(
            lines(Some("installed from npm")).lines().nth(1),
            Some("installed from npm")
        );
        assert_eq!(lines(Some("")).lines().nth(1), lines(None).lines().nth(1));
    }

    #[test]
    fn targets_have_the_names_of_the_release_archives() {
        assert_eq!(go_os("macos"), "darwin");
        assert_eq!(go_os("linux"), "linux");
        assert_eq!(go_os("windows"), "windows");
        assert_eq!(go_arch("x86_64"), "amd64");
        assert_eq!(go_arch("aarch64"), "arm64");
    }
}
