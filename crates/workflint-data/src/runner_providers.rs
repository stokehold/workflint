//! The labels of third-party runner providers, the operating system a label names, and what a
//! job's `runs-on` resolves to.
//!
//! The reference knows no provider labels. Each provider below lists its label formats as its
//! documentation gave them on the retrieval date, and only providers that document a format are
//! here (`docs/differences.md#runner-provider-labels`). Until M7.3 generates runner label data
//! (ADR 0011), update the patterns by hand from the same pages.

use crate::go_case::{eq_go_fold, eq_go_lower, starts_with_go_lower};
use crate::runner_labels::{
    GITHUB_HOSTED_RUNNER_LABELS, SELF_HOSTED_RUNNER_OS_LABELS, is_self_hosted_other_label,
};

/// The operating system of a runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOs {
    /// Linux, which includes Ubuntu.
    Linux,
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
}

/// A company that hosts runners, which a job selects with the provider's labels.
#[derive(Debug, PartialEq, Eq)]
pub struct RunnerProvider {
    /// The provider's name.
    pub name: &'static str,
    /// The documentation page of the provider's labels.
    pub source: &'static str,
    /// The date the labels were copied from [`Self::source`], as `YYYY-MM-DD`.
    pub retrieved: &'static str,
    labels: ProviderLabels,
}

#[derive(Debug, PartialEq, Eq)]
enum ProviderLabels {
    /// Label patterns and the system each names. In a pattern, `#` matches one or more ASCII
    /// digits, `*` matches one or more characters, and any other character matches itself
    /// without ASCII case.
    Patterns(&'static [(&'static str, Option<RunnerOs>)]),
    /// Labels made of `key=value` parts separated by `/` or `,`, where every key is one of these.
    KeyValue(&'static [&'static str]),
}

/// A label of a provider and the system it names, if the label alone tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderLabel {
    /// The provider whose runners have the label.
    pub provider: &'static RunnerProvider,
    /// The system the label names. A label with `None`, such as `runs-on=1/cpu=2`, can still name
    /// one in its words, which [`runner_label_os`] finds.
    pub os: Option<RunnerOs>,
}

const LINUX: Option<RunnerOs> = Some(RunnerOs::Linux);
const MACOS: Option<RunnerOs> = Some(RunnerOs::MacOs);
const WINDOWS: Option<RunnerOs> = Some(RunnerOs::Windows);

/// The providers whose label formats are documented, by name.
pub static RUNNER_PROVIDERS: &[RunnerProvider] = &[
    RunnerProvider {
        name: "Blacksmith",
        source: "https://docs.blacksmith.sh/blacksmith-runners/overview",
        retrieved: "2026-10-19",
        labels: ProviderLabels::Patterns(&[
            ("blacksmith-#vcpu-ubuntu-#", LINUX),
            ("blacksmith-#vcpu-ubuntu-#-arm", LINUX),
            ("blacksmith-#vcpu-windows-#", WINDOWS),
            ("blacksmith-#vcpu-macos-#", MACOS),
            ("blacksmith-#vcpu-macos-latest", MACOS),
        ]),
    },
    RunnerProvider {
        name: "Depot",
        source: "https://depot.dev/docs/github-actions/runner-types",
        retrieved: "2026-10-19",
        labels: ProviderLabels::Patterns(&[
            ("depot-ubuntu-#.#", LINUX),
            ("depot-ubuntu-#.#-#", LINUX),
            ("depot-ubuntu-#.#-arm", LINUX),
            ("depot-ubuntu-#.#-arm-#", LINUX),
            ("depot-ubuntu-latest", LINUX),
            ("depot-windows-#", WINDOWS),
            ("depot-windows-#-#", WINDOWS),
            ("depot-windows-latest", WINDOWS),
            ("depot-macos-#", MACOS),
            ("depot-macos-latest", MACOS),
        ]),
    },
    RunnerProvider {
        name: "Namespace",
        source: "https://namespace.so/docs/reference/github-actions/runner-configuration",
        retrieved: "2026-10-19",
        // `nscloud-{os}-{arch}-{shape}` with optional `-with-*` suffixes. The OS names of macOS
        // contain `-`, as `macos-tahoe-slim` does, so the rest of the label is not checked.
        labels: ProviderLabels::Patterns(&[
            ("nscloud-ubuntu-*", LINUX),
            ("nscloud-windows-*", WINDOWS),
            ("nscloud-macos-*", MACOS),
            ("namespace-profile-*", None),
            ("namespace-features:*", None),
        ]),
    },
    RunnerProvider {
        name: "RunsOn",
        source: "https://runs-on.com/configuration/job-labels/",
        retrieved: "2026-10-19",
        labels: ProviderLabels::KeyValue(&[
            "runs-on",
            "ami",
            "cpu",
            "debug",
            "disk",
            "env",
            "extras",
            "family",
            "image",
            "nested-virt",
            "pool",
            "private",
            "ram",
            "region",
            "retry",
            "runner",
            "spot",
            "ssh",
            "sticky",
            "volume",
        ]),
    },
    RunnerProvider {
        name: "Ubicloud",
        source: "https://www.ubicloud.com/docs/github-actions-integration/runner-types",
        retrieved: "2026-10-19",
        labels: ProviderLabels::Patterns(&[
            ("ubicloud", LINUX),
            ("ubicloud-arm", LINUX),
            ("ubicloud-standard-#", LINUX),
            ("ubicloud-standard-#-arm", LINUX),
            ("ubicloud-standard-#-ubuntu-#", LINUX),
            ("ubicloud-standard-#-arm-ubuntu-#", LINUX),
        ]),
    },
    RunnerProvider {
        name: "WarpBuild",
        source: "https://www.warpbuild.com/docs/ci/cloud-runners",
        retrieved: "2026-10-19",
        labels: ProviderLabels::Patterns(&[
            ("warp-ubuntu-latest-x64-#x", LINUX),
            ("warp-ubuntu-#-x64-#x", LINUX),
            ("warp-ubuntu-latest-arm64-#x", LINUX),
            ("warp-ubuntu-#-arm64-#x", LINUX),
            ("warp-ubuntu-#-arm64-#x-xfast", LINUX),
            ("warp-windows-latest-x64-#x", WINDOWS),
            ("warp-windows-#-x64-#x", WINDOWS),
            ("warp-windows-#-vs#-x64-#x", WINDOWS),
            ("warp-macos-latest-arm64-#x", MACOS),
            ("warp-macos-#-arm64-#x", MACOS),
        ]),
    },
];

/// Returns the provider whose runners have `label`, and the system the label names. Labels
/// compare without ASCII case.
#[must_use]
pub fn runner_provider(label: &str) -> Option<ProviderLabel> {
    RUNNER_PROVIDERS.iter().find_map(|provider| {
        let os = match provider.labels {
            ProviderLabels::Patterns(patterns) => patterns
                .iter()
                .find(|(pattern, _)| matches_pattern(pattern.as_bytes(), label.as_bytes()))
                .map(|(_, os)| *os)?,
            ProviderLabels::KeyValue(keys) => is_key_value_label(label, keys).then_some(None)?,
        };
        Some(ProviderLabel { provider, os })
    })
}

/// Returns whether `text` matches `pattern`, as [`ProviderLabels::Patterns`] describes.
fn matches_pattern(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'#', rest)) => {
            let digits = text.iter().take_while(|c| c.is_ascii_digit()).count();
            (1..=digits).any(|n| matches_pattern(rest, &text[n..]))
        }
        Some((b'*', rest)) => (1..=text.len()).any(|n| matches_pattern(rest, &text[n..])),
        Some((c, rest)) => text
            .split_first()
            .is_some_and(|(t, text)| t.eq_ignore_ascii_case(c) && matches_pattern(rest, text)),
    }
}

/// Returns whether every `/` or `,` separated part of `label` is `key=value` with a key from
/// `keys`.
fn is_key_value_label(label: &str, keys: &[&str]) -> bool {
    label.split(['/', ',']).all(|part| {
        part.split_once('=')
            .is_some_and(|(key, _)| keys.iter().any(|known| key.eq_ignore_ascii_case(known)))
    })
}

/// Returns the system that `label` names, or `None` when it names none or more than one.
///
/// The reference's prefixes decide first, as its `getPlatformFromRunner` does: `windows` and
/// `windows-*` are Windows, `macos` and `macos-*` macOS, and `linux` and `ubuntu-*` Linux. A
/// provider label names the system of its provider's pattern. Any other label names the systems
/// of its words, where a word is a run of ASCII letters, digits, and `.`, and the words `linux`,
/// `ubuntu`, `macos`, and `windows`, alone or followed by a version such as `2022` or `22.04`,
/// name their system. So `self-32vcpu-windows-2022` is Windows, and `windowsill` names none.
///
/// A caller that has a label with `${{ }}` passes it with each `${{ }}` replaced by a space, so
/// that the words of the expression do not count.
#[must_use]
pub fn runner_label_os(label: &str) -> Option<RunnerOs> {
    if let Some(os) = reference_label_os(label) {
        return Some(os);
    }
    if let Some(ProviderLabel { os: Some(os), .. }) = runner_provider(label) {
        return Some(os);
    }
    let mut words = label
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
        .filter_map(word_os);
    let first = words.next()?;
    words.all(|os| os == first).then_some(first)
}

/// The reference's `getPlatformFromRunner` for one label. The reference does not tell macOS
/// from Linux. This does.
fn reference_label_os(label: &str) -> Option<RunnerOs> {
    let starts_with = |prefix| starts_with_go_lower(label, prefix);
    if eq_go_lower(label, "windows") || starts_with("windows-") {
        Some(RunnerOs::Windows)
    } else if eq_go_lower(label, "macos") || starts_with("macos-") {
        Some(RunnerOs::MacOs)
    } else if eq_go_lower(label, "linux") || starts_with("ubuntu-") {
        Some(RunnerOs::Linux)
    } else {
        None
    }
}

/// Returns the system a word names, such as `windows2022` or `ubuntu22.04`.
fn word_os(word: &str) -> Option<RunnerOs> {
    [
        ("linux", RunnerOs::Linux),
        ("ubuntu", RunnerOs::Linux),
        ("macos", RunnerOs::MacOs),
        ("windows", RunnerOs::Windows),
    ]
    .into_iter()
    .find_map(|(name, os)| {
        let version = word
            .get(..name.len())
            .filter(|start| start.eq_ignore_ascii_case(name))
            .and_then(|_| word.get(name.len()..))?;
        let is_version = version.is_empty()
            || (version.starts_with(|c: char| c.is_ascii_digit())
                && version.chars().all(|c| c.is_ascii_digit() || c == '.'));
        is_version.then_some(os)
    })
}

/// Who hosts the runner that a job's `runs-on` selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerKind {
    /// A GitHub-hosted runner, or with `provider`, a runner of a third-party provider.
    Hosted {
        /// The provider, or `None` for GitHub.
        provider: Option<&'static RunnerProvider>,
    },
    /// A self-hosted runner: a label is `self-hosted`, or a label that only self-hosted runners
    /// get, such as `linux` or `arm64`.
    SelfHosted,
    /// The labels do not tell, because they are custom labels or come from an expression.
    Unknown,
}

/// What a job's `runs-on` resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedRunner {
    /// Who hosts the runner.
    pub kind: RunnerKind,
    /// The runner's system, when the labels name one and only one.
    pub os: Option<RunnerOs>,
}

/// Resolves the labels of a job's `runs-on`. A label is `None` when its text is not known, as
/// for a whole `runs-on: ${{ }}`. A label with `${{ }}` inside is passed as
/// [`runner_label_os`] says.
///
/// A `self-hosted` label makes the runner self-hosted. Otherwise a GitHub-hosted or provider
/// label makes it hosted, by the first such label, and a self-hosted label for a system or an
/// architecture makes it self-hosted. The system is the one that the labels name, or `None` when
/// they name none or two.
#[must_use]
pub fn resolve_runner<'a>(labels: impl IntoIterator<Item = Option<&'a str>>) -> ResolvedRunner {
    let mut self_hosted = false;
    let mut hosted = None;
    let mut preset = false;
    let mut os = None;
    let mut os_conflict = false;
    for label in labels.into_iter().flatten() {
        if eq_go_fold(label, "self-hosted") {
            self_hosted = true;
        } else if hosted.is_none() {
            if GITHUB_HOSTED_RUNNER_LABELS
                .iter()
                .any(|known| eq_go_fold(label, known))
            {
                hosted = Some(None);
            } else if let Some(found) = runner_provider(label) {
                hosted = Some(Some(found.provider));
            }
        }
        if is_self_hosted_other_label(label)
            || SELF_HOSTED_RUNNER_OS_LABELS
                .iter()
                .any(|known| eq_go_fold(label, known))
        {
            preset = true;
        }
        if let Some(named) = runner_label_os(label) {
            os_conflict |= os.is_some_and(|os| os != named);
            os = Some(named);
        }
    }
    let kind = if self_hosted {
        RunnerKind::SelfHosted
    } else if let Some(provider) = hosted {
        RunnerKind::Hosted { provider }
    } else if preset {
        RunnerKind::SelfHosted
    } else {
        RunnerKind::Unknown
    };
    ResolvedRunner {
        kind,
        os: if os_conflict { None } else { os },
    }
}
