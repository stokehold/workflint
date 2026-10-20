//! The runner labels that `runs-on` knows without configuration, and the operating systems that
//! each one can run on.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `runnerOSCompat`, `allGitHubHostedRunnerLabels`,
//! `selfHostedRunnerPresetOSLabels`, `selfHostedRunnerPresetOtherLabels`, and
//! `defaultRunnerOSCompats` in `rule_runner_label.go`, lines 8-112. Until M7.3 replaces them
//! with a generated file (ADR 0011), keep them equal to that code plus the updates from GitHub's
//! documentation and changelog below (ADR 0033, `docs/differences.md#platform-data-updates`).
//! The tests in `tests/runner_labels.rs` compare them.
//!
//! Checked on 2026-10-20:
//!
//! - <https://docs.github.com/en/actions/reference/runners/github-hosted-runners> lists
//!   `ubuntu-26.04`, `ubuntu-26.04-arm`, `windows-11-vs2026-arm`, and `xcode-27`, and maps
//!   `windows-latest` to Windows Server 2025 and `macos-latest` to macOS 26.
//! - <https://docs.github.com/en/actions/reference/runners/larger-runners> lists
//!   `xcode-27-xlarge`, and marks `macos-15-large` and `macos-15-xlarge` as the latest large and
//!   extra-large macOS runners.
//! - <https://github.blog/changelog/2026-10-01-github-actions-macos-14-runner-image-retirement/>
//!   maps `macos-latest-xlarge` to `macos-26-xlarge`. The sources disagree, so
//!   `macos-latest-xlarge` can be either.
//! - <https://github.blog/changelog/2026-09-17-ubuntu-26-generally-available-and-latest-migration/>
//!   moves `ubuntu-latest` from Ubuntu 24.04 to 26.04 between 2026-10-19 and 2026-11-19, so it
//!   can be either.
//! - <https://github.blog/changelog/2026-05-14-github-actions-upcoming-image-migrations/> moves
//!   `windows-2025` to Visual Studio 2026 and points `windows-2025-vs2026` at the same image.
//! - <https://github.blog/changelog/2026-08-20-windows-11-arm64-vs2026-image-generally-available/>
//!   moves `windows-11-arm` to the image of `windows-11-vs2026-arm` by 2026-09-30.
//! - <https://github.blog/changelog/2026-10-15-xcode-27-runner-image-generally-available/> says
//!   that `xcode-27` and `xcode-27-xlarge` run macOS 27 on arm64.
//! - <https://docs.github.com/en/actions/hosting-your-own-runners/using-self-hosted-runners-in-a-workflow#using-default-labels-to-route-jobs>

use crate::go_case::{eq_go_fold, eq_go_lower};

/// A set of runner images that a label can run on. Two labels of one job conflict when their
/// sets do not intersect, as `ubuntu-latest` and `windows-latest` do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunnerOsCompat(u32);

impl RunnerOsCompat {
    /// Ubuntu 22.04.
    pub const UBUNTU_22_04: Self = Self(1 << 0);
    /// Ubuntu 24.04.
    pub const UBUNTU_24_04: Self = Self(1 << 1);
    /// Ubuntu 26.04.
    pub const UBUNTU_26_04: Self = Self(1 << 2);
    /// macOS 14 on a standard runner.
    pub const MACOS_14: Self = Self(1 << 3);
    /// macOS 14 on a large runner.
    pub const MACOS_14_LARGE: Self = Self(1 << 4);
    /// macOS 14 on an extra-large runner.
    pub const MACOS_14_XLARGE: Self = Self(1 << 5);
    /// macOS 15 on a standard runner.
    pub const MACOS_15: Self = Self(1 << 6);
    /// macOS 15 on an Intel runner.
    pub const MACOS_15_INTEL: Self = Self(1 << 7);
    /// macOS 15 on a large runner.
    pub const MACOS_15_LARGE: Self = Self(1 << 8);
    /// macOS 15 on an extra-large runner.
    pub const MACOS_15_XLARGE: Self = Self(1 << 9);
    /// macOS 26 on a standard runner.
    pub const MACOS_26: Self = Self(1 << 10);
    /// macOS 26 on an Intel runner.
    pub const MACOS_26_INTEL: Self = Self(1 << 11);
    /// macOS 26 on a large runner.
    pub const MACOS_26_LARGE: Self = Self(1 << 12);
    /// macOS 26 on an extra-large runner.
    pub const MACOS_26_XLARGE: Self = Self(1 << 13);
    /// Windows Server 2022.
    pub const WINDOWS_2022: Self = Self(1 << 14);
    /// Windows Server 2025, which has had Visual Studio 2026 since June 2026.
    pub const WINDOWS_2025: Self = Self(1 << 15);
    /// Windows 11 on Arm.
    pub const WINDOWS_11_ARM: Self = Self(1 << 16);
    /// The Xcode 27 image, on macOS 27, on a standard runner.
    pub const XCODE_27: Self = Self(1 << 17);
    /// The Xcode 27 image, on macOS 27, on an extra-large runner.
    pub const XCODE_27_XLARGE: Self = Self(1 << 18);

    /// The set as bits, one per image.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The images of either set.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns whether the sets share an image, so that one runner can have both labels.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

/// A label and the images it can run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunnerLabelOsCompat {
    /// The label in lower case.
    pub name: &'static str,
    /// The images that a runner with the label can have.
    pub os: RunnerOsCompat,
}

const fn label(name: &'static str, os: RunnerOsCompat) -> RunnerLabelOsCompat {
    RunnerLabelOsCompat { name, os }
}

/// The labels of GitHub-hosted runners, in the order that the reference's message lists them.
/// Each added label follows the labels of its system.
pub static GITHUB_HOSTED_RUNNER_LABELS: &[&str] = &[
    "windows-latest",
    "windows-latest-8-cores",
    "windows-2025",
    "windows-2025-vs2026",
    "windows-2022",
    "windows-11-arm",
    "windows-11-vs2026-arm",
    "ubuntu-slim",
    "ubuntu-latest",
    "ubuntu-latest-4-cores",
    "ubuntu-latest-8-cores",
    "ubuntu-latest-16-cores",
    "ubuntu-26.04",
    "ubuntu-26.04-arm",
    "ubuntu-24.04",
    "ubuntu-24.04-arm",
    "ubuntu-22.04",
    "ubuntu-22.04-arm",
    "macos-latest",
    "macos-latest-xlarge",
    "macos-latest-large",
    "xcode-27-xlarge",
    "xcode-27",
    "macos-26-intel",
    "macos-26-xlarge",
    "macos-26-large",
    "macos-26",
    "macos-15-intel",
    "macos-15-xlarge",
    "macos-15-large",
    "macos-15",
    "macos-14-xlarge",
    "macos-14-large",
    "macos-14",
];

/// The labels that a self-hosted runner gets for its operating system.
pub static SELF_HOSTED_RUNNER_OS_LABELS: &[&str] = &["linux", "macos", "windows"];

/// The other labels that a self-hosted runner gets: `self-hosted` and its architecture.
pub static SELF_HOSTED_RUNNER_OTHER_LABELS: &[&str] = &["self-hosted", "x64", "arm", "arm64"];

const MACOS: RunnerOsCompat = RunnerOsCompat::MACOS_26
    .union(RunnerOsCompat::MACOS_26_INTEL)
    .union(RunnerOsCompat::MACOS_26_LARGE)
    .union(RunnerOsCompat::MACOS_26_XLARGE)
    .union(RunnerOsCompat::MACOS_15)
    .union(RunnerOsCompat::MACOS_15_INTEL)
    .union(RunnerOsCompat::MACOS_15_LARGE)
    .union(RunnerOsCompat::MACOS_15_XLARGE)
    .union(RunnerOsCompat::MACOS_14)
    .union(RunnerOsCompat::MACOS_14_LARGE)
    .union(RunnerOsCompat::MACOS_14_XLARGE)
    .union(RunnerOsCompat::XCODE_27)
    .union(RunnerOsCompat::XCODE_27_XLARGE);

const WINDOWS: RunnerOsCompat = RunnerOsCompat::WINDOWS_2025
    .union(RunnerOsCompat::WINDOWS_2022)
    .union(RunnerOsCompat::WINDOWS_11_ARM);

/// The images of every GitHub-hosted label and of the self-hosted operating system labels, in
/// the reference's order.
pub static RUNNER_LABEL_OS_COMPATS: &[RunnerLabelOsCompat] = &[
    label("ubuntu-slim", RunnerOsCompat::UBUNTU_24_04),
    label(
        "ubuntu-latest",
        RunnerOsCompat::UBUNTU_24_04.union(RunnerOsCompat::UBUNTU_26_04),
    ),
    label("ubuntu-latest-4-cores", RunnerOsCompat::UBUNTU_24_04),
    label("ubuntu-latest-8-cores", RunnerOsCompat::UBUNTU_24_04),
    label("ubuntu-latest-16-cores", RunnerOsCompat::UBUNTU_24_04),
    label("ubuntu-26.04", RunnerOsCompat::UBUNTU_26_04),
    label("ubuntu-26.04-arm", RunnerOsCompat::UBUNTU_26_04),
    label("ubuntu-24.04", RunnerOsCompat::UBUNTU_24_04),
    label("ubuntu-24.04-arm", RunnerOsCompat::UBUNTU_24_04),
    label("ubuntu-22.04", RunnerOsCompat::UBUNTU_22_04),
    label("ubuntu-22.04-arm", RunnerOsCompat::UBUNTU_22_04),
    label(
        "macos-latest-xlarge",
        RunnerOsCompat::MACOS_15_XLARGE.union(RunnerOsCompat::MACOS_26_XLARGE),
    ),
    label("macos-latest-large", RunnerOsCompat::MACOS_15_LARGE),
    label("macos-latest", RunnerOsCompat::MACOS_26),
    label("xcode-27-xlarge", RunnerOsCompat::XCODE_27_XLARGE),
    label("xcode-27", RunnerOsCompat::XCODE_27),
    label("macos-26-intel", RunnerOsCompat::MACOS_26_INTEL),
    label("macos-26-xlarge", RunnerOsCompat::MACOS_26_XLARGE),
    label("macos-26-large", RunnerOsCompat::MACOS_26_LARGE),
    label("macos-26", RunnerOsCompat::MACOS_26),
    label("macos-15-intel", RunnerOsCompat::MACOS_15_INTEL),
    label("macos-15-xlarge", RunnerOsCompat::MACOS_15_XLARGE),
    label("macos-15-large", RunnerOsCompat::MACOS_15_LARGE),
    label("macos-15", RunnerOsCompat::MACOS_15),
    label("macos-14-xlarge", RunnerOsCompat::MACOS_14_XLARGE),
    label("macos-14-large", RunnerOsCompat::MACOS_14_LARGE),
    label("macos-14", RunnerOsCompat::MACOS_14),
    label("windows-latest", RunnerOsCompat::WINDOWS_2025),
    label("windows-latest-8-cores", RunnerOsCompat::WINDOWS_2022),
    label("windows-2025", RunnerOsCompat::WINDOWS_2025),
    label("windows-2025-vs2026", RunnerOsCompat::WINDOWS_2025),
    label("windows-2022", RunnerOsCompat::WINDOWS_2022),
    label("windows-11-arm", RunnerOsCompat::WINDOWS_11_ARM),
    label("windows-11-vs2026-arm", RunnerOsCompat::WINDOWS_11_ARM),
    // `linux` does not always mean Ubuntu, but the reference assumes it does.
    label(
        "linux",
        RunnerOsCompat::UBUNTU_26_04
            .union(RunnerOsCompat::UBUNTU_24_04)
            .union(RunnerOsCompat::UBUNTU_22_04),
    ),
    label("macos", MACOS),
    label("windows", WINDOWS),
];

/// Returns the images that `label` can run on, comparing as the reference does after Go's
/// `strings.ToLower`. Labels without an image, such as `self-hosted` or a custom label, have
/// none.
#[must_use]
pub fn runner_label_os_compat(label: &str) -> Option<RunnerOsCompat> {
    RUNNER_LABEL_OS_COMPATS
        .iter()
        .find(|known| eq_go_lower(label, known.name))
        .map(|known| known.os)
}

/// Returns whether `label` is one of [`SELF_HOSTED_RUNNER_OTHER_LABELS`], comparing as Go's
/// `strings.EqualFold` does.
#[must_use]
pub fn is_self_hosted_other_label(label: &str) -> bool {
    SELF_HOSTED_RUNNER_OTHER_LABELS
        .iter()
        .any(|known| eq_go_fold(label, known))
}
