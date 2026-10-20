//! Checks the runner labels against the reference's tables in the vendored
//! `rule_runner_label.go`, with the changes and additions that workflint makes
//! (`docs/differences.md#platform-data-updates`).

#![allow(clippy::panic)]

mod go_source;

use go_source::{
    all_github_hosted_runner_labels, default_runner_os_compats, runner_os_compat_bits,
    self_hosted_runner_preset_os_labels, self_hosted_runner_preset_other_labels,
};
use workflint_data::{
    GITHUB_HOSTED_RUNNER_LABELS, RUNNER_LABEL_OS_COMPATS, RunnerOsCompat,
    SELF_HOSTED_RUNNER_OS_LABELS, SELF_HOSTED_RUNNER_OTHER_LABELS, is_self_hosted_other_label,
    runner_label_os_compat,
};

/// Each bit of the reference's `runnerOSCompat`, by its Go name, in declaration order, and the
/// images that workflint gives it.
const BITS: [(&str, RunnerOsCompat); 17] = [
    ("compatUbuntu2204", RunnerOsCompat::UBUNTU_22_04),
    ("compatUbuntu2404", RunnerOsCompat::UBUNTU_24_04),
    ("compatMacOS140", RunnerOsCompat::MACOS_14),
    ("compatMacOS140L", RunnerOsCompat::MACOS_14_LARGE),
    ("compatMacOS140XL", RunnerOsCompat::MACOS_14_XLARGE),
    ("compatMacOS150", RunnerOsCompat::MACOS_15),
    ("compatMacOS150Intel", RunnerOsCompat::MACOS_15_INTEL),
    ("compatMacOS150L", RunnerOsCompat::MACOS_15_LARGE),
    ("compatMacOS150XL", RunnerOsCompat::MACOS_15_XLARGE),
    ("compatMacOS260", RunnerOsCompat::MACOS_26),
    ("compatMacOS260Intel", RunnerOsCompat::MACOS_26_INTEL),
    ("compatMacOS260L", RunnerOsCompat::MACOS_26_LARGE),
    ("compatMacOS260XL", RunnerOsCompat::MACOS_26_XLARGE),
    ("compatWindows2022", RunnerOsCompat::WINDOWS_2022),
    ("compatWindows2025", RunnerOsCompat::WINDOWS_2025),
    // GitHub moved `windows-2025` to Visual Studio 2026 in June 2026, and `windows-2025-vs2026`
    // now points to the same image.
    ("compatWindows2025VS2026", RunnerOsCompat::WINDOWS_2025),
    ("compatWindows11Arm", RunnerOsCompat::WINDOWS_11_ARM),
];

/// The images that workflint adds to the reference's bits.
const ADDED_BITS: [RunnerOsCompat; 3] = [
    RunnerOsCompat::UBUNTU_26_04,
    RunnerOsCompat::XCODE_27,
    RunnerOsCompat::XCODE_27_XLARGE,
];

const MACOS_XLARGE_LATEST: RunnerOsCompat =
    RunnerOsCompat::MACOS_15_XLARGE.union(RunnerOsCompat::MACOS_26_XLARGE);

/// The reference's labels whose images workflint changes, with the new images.
const CHANGED_LABELS: [(&str, RunnerOsCompat); 6] = [
    (
        "ubuntu-latest",
        RunnerOsCompat::UBUNTU_24_04.union(RunnerOsCompat::UBUNTU_26_04),
    ),
    ("macos-latest-xlarge", MACOS_XLARGE_LATEST),
    ("macos-latest", RunnerOsCompat::MACOS_26),
    ("windows-latest", RunnerOsCompat::WINDOWS_2025),
    (
        "linux",
        RunnerOsCompat::UBUNTU_26_04
            .union(RunnerOsCompat::UBUNTU_24_04)
            .union(RunnerOsCompat::UBUNTU_22_04),
    ),
    ("macos", MACOS_WITH_XCODE),
];

/// The images of the self-hosted `macos` label in the reference, plus the Xcode images.
const MACOS_WITH_XCODE: RunnerOsCompat = RunnerOsCompat::MACOS_26
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

/// The GitHub-hosted labels that workflint adds, with their images.
const ADDED_LABELS: [(&str, RunnerOsCompat); 5] = [
    ("ubuntu-26.04", RunnerOsCompat::UBUNTU_26_04),
    ("ubuntu-26.04-arm", RunnerOsCompat::UBUNTU_26_04),
    ("xcode-27-xlarge", RunnerOsCompat::XCODE_27_XLARGE),
    ("xcode-27", RunnerOsCompat::XCODE_27),
    ("windows-11-vs2026-arm", RunnerOsCompat::WINDOWS_11_ARM),
];

/// A label and the bits of its images.
type LabelBits = (String, u32);

fn is_added(label: &str) -> bool {
    ADDED_LABELS.iter().any(|(added, _)| *added == label)
}

fn owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn compatibility_bits_match_the_reference_constants() {
    let ours: Vec<&str> = BITS.iter().map(|(go, _)| *go).collect();
    assert_eq!(ours, runner_os_compat_bits());
}

#[test]
fn compatibility_bits_are_distinct_single_bits() {
    let mut bits: Vec<RunnerOsCompat> = BITS.iter().map(|(_, bit)| *bit).collect();
    bits.dedup();
    bits.extend(ADDED_BITS);
    let mut seen = 0_u32;
    for bit in bits {
        assert_eq!(bit.bits().count_ones(), 1, "{bit:?}");
        assert_eq!(seen & bit.bits(), 0, "{bit:?} reuses a bit");
        seen |= bit.bits();
    }
}

#[test]
fn github_hosted_labels_are_the_reference_list_and_the_added_labels() {
    let (added, reference): (Vec<&str>, Vec<&str>) = GITHUB_HOSTED_RUNNER_LABELS
        .iter()
        .partition(|label| is_added(label));
    assert_eq!(owned(&reference), all_github_hosted_runner_labels());
    let expected: Vec<&str> = ADDED_LABELS.iter().map(|(label, _)| *label).collect();
    let mut added_sorted = added;
    added_sorted.sort_unstable();
    let mut expected_sorted = expected;
    expected_sorted.sort_unstable();
    assert_eq!(added_sorted, expected_sorted);
}

#[test]
fn added_labels_follow_the_labels_of_the_same_system() {
    let position = |label: &str| {
        GITHUB_HOSTED_RUNNER_LABELS
            .iter()
            .position(|known| *known == label)
    };
    assert_eq!(
        position("windows-11-vs2026-arm"),
        position("windows-11-arm").map(|i| i + 1)
    );
    assert_eq!(
        position("ubuntu-26.04"),
        position("ubuntu-24.04").map(|i| i - 2)
    );
    assert_eq!(
        position("ubuntu-26.04-arm"),
        position("ubuntu-24.04").map(|i| i - 1)
    );
    assert_eq!(
        position("xcode-27-xlarge"),
        position("macos-latest-large").map(|i| i + 1)
    );
    assert_eq!(
        position("xcode-27"),
        position("macos-latest-large").map(|i| i + 2)
    );
}

#[test]
fn self_hosted_presets_match_the_reference_lists() {
    assert_eq!(
        owned(SELF_HOSTED_RUNNER_OS_LABELS),
        self_hosted_runner_preset_os_labels()
    );
    assert_eq!(
        owned(SELF_HOSTED_RUNNER_OTHER_LABELS),
        self_hosted_runner_preset_other_labels()
    );
}

#[test]
fn os_compatibility_is_the_reference_table_with_the_changed_and_added_labels() {
    let (added, ours): (Vec<LabelBits>, Vec<LabelBits>) = RUNNER_LABEL_OS_COMPATS
        .iter()
        .map(|label| (label.name.to_owned(), label.os.bits()))
        .partition(|(name, _)| is_added(name));
    let expected_added: Vec<LabelBits> = ADDED_LABELS
        .iter()
        .map(|(name, os)| ((*name).to_owned(), os.bits()))
        .collect();
    let mut added = added;
    added.sort_unstable();
    let mut expected_added = expected_added;
    expected_added.sort_unstable();
    assert_eq!(added, expected_added);

    let reference: Vec<LabelBits> = default_runner_os_compats()
        .into_iter()
        .map(|(name, bits)| {
            let value = bits.iter().fold(0, |value, go| {
                let Some((_, bit)) = BITS.iter().find(|(name, _)| name == go) else {
                    panic!("{go} is not a bit");
                };
                value | bit.bits()
            });
            let value = CHANGED_LABELS
                .iter()
                .find(|(changed, _)| *changed == name)
                .map_or(value, |(_, os)| os.bits());
            (name, value)
        })
        .collect();
    assert_eq!(ours, reference);
}

#[test]
fn every_changed_label_is_in_the_reference_table() {
    let reference = default_runner_os_compats();
    for (label, _) in CHANGED_LABELS {
        assert!(
            reference.iter().any(|(name, _)| name == label),
            "{label} is not in the reference table"
        );
    }
}

#[test]
fn latest_labels_share_an_image_with_the_version_github_maps_them_to() {
    let conflict = |a: &str, b: &str| {
        let (Some(a_os), Some(b_os)) = (runner_label_os_compat(a), runner_label_os_compat(b))
        else {
            panic!("{a} or {b} has no images");
        };
        !a_os.intersects(b_os)
    };
    assert!(!conflict("windows-latest", "windows-2025"));
    assert!(conflict("windows-latest", "windows-2022"));
    assert!(!conflict("windows-latest", "windows-2025-vs2026"));
    assert!(!conflict("windows-11-arm", "windows-11-vs2026-arm"));
    assert!(!conflict("macos-latest", "macos-26"));
    assert!(conflict("macos-latest", "macos-15"));
    assert!(!conflict("macos-latest-xlarge", "macos-26-xlarge"));
    assert!(!conflict("macos-latest-xlarge", "macos-15-xlarge"));
    assert!(!conflict("macos-latest-large", "macos-15-large"));
    // `ubuntu-latest` moves from 24.04 to 26.04 between 2026-10-19 and 2026-11-19.
    assert!(!conflict("ubuntu-latest", "ubuntu-24.04"));
    assert!(!conflict("ubuntu-latest", "ubuntu-26.04"));
    assert!(conflict("ubuntu-26.04", "ubuntu-24.04"));
    assert!(!conflict("ubuntu-26.04-arm", "linux"));
    assert!(!conflict("xcode-27", "macos"));
    assert!(conflict("xcode-27", "macos-26"));
    assert!(conflict("xcode-27", "xcode-27-xlarge"));
}

#[test]
fn os_compatibility_lookup_lowers_the_case_as_go_does() {
    assert_eq!(
        runner_label_os_compat("Ubuntu-24.04"),
        Some(RunnerOsCompat::UBUNTU_24_04)
    );
    // Go's `strings.ToLower` maps `İ` to `i`, and the Kelvin sign to `k`.
    assert_eq!(
        runner_label_os_compat("w\u{130}ndows-2022"),
        Some(RunnerOsCompat::WINDOWS_2022)
    );
    let Some(linux) = runner_label_os_compat("LINUX") else {
        panic!("linux has no OS compatibility");
    };
    assert!(linux.intersects(RunnerOsCompat::UBUNTU_22_04));
    assert!(!linux.intersects(RunnerOsCompat::WINDOWS_2022));
    assert_eq!(runner_label_os_compat("self-hosted"), None);
    assert_eq!(runner_label_os_compat("ubuntu-latest "), None);
}

#[test]
fn other_self_hosted_labels_compare_as_go_equal_fold_does() {
    assert!(is_self_hosted_other_label("Self-Hosted"));
    assert!(is_self_hosted_other_label("ARM64"));
    // `strings.EqualFold` matches `ſ` with `s`, but not `İ` with `i`.
    assert!(is_self_hosted_other_label("\u{17f}elf-hosted"));
    assert!(!is_self_hosted_other_label("linux"));
    assert!(!is_self_hosted_other_label("x64 "));
}
