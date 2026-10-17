//! Checks the runner labels against the reference's tables in the vendored
//! `rule_runner_label.go`.

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

/// Each bit of the reference's `runnerOSCompat`, by its Go name, in declaration order.
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
    (
        "compatWindows2025VS2026",
        RunnerOsCompat::WINDOWS_2025_VS2026,
    ),
    ("compatWindows11Arm", RunnerOsCompat::WINDOWS_11_ARM),
];

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
    let mut seen = 0_u32;
    for (go, bit) in BITS {
        assert_eq!(bit.bits().count_ones(), 1, "{go}");
        assert_eq!(seen & bit.bits(), 0, "{go} reuses a bit");
        seen |= bit.bits();
    }
}

#[test]
fn github_hosted_labels_match_the_reference_list_in_message_order() {
    assert_eq!(
        owned(GITHUB_HOSTED_RUNNER_LABELS),
        all_github_hosted_runner_labels()
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
fn os_compatibility_of_each_label_matches_the_reference_table() {
    let ours: Vec<(String, u32)> = RUNNER_LABEL_OS_COMPATS
        .iter()
        .map(|label| (label.name.to_owned(), label.os.bits()))
        .collect();
    let reference: Vec<(String, u32)> = default_runner_os_compats()
        .into_iter()
        .map(|(name, bits)| {
            let value = bits.iter().fold(0, |value, go| {
                let Some((_, bit)) = BITS.iter().find(|(name, _)| name == go) else {
                    panic!("{go} is not a bit");
                };
                value | bit.bits()
            });
            (name, value)
        })
        .collect();
    assert_eq!(ours, reference);
}

#[test]
fn os_compatibility_lookup_lowers_the_case_as_go_does() {
    assert_eq!(
        runner_label_os_compat("Ubuntu-Latest"),
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
