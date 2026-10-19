//! Checks the labels of third-party runner providers and how a job's `runs-on` resolves.

use workflint_data::{
    RUNNER_PROVIDERS, RunnerKind, RunnerOs, resolve_runner, runner_label_os, runner_provider,
};

/// Returns the name of the provider of `label` and the OS the label implies.
fn provider(label: &str) -> Option<(&'static str, Option<RunnerOs>)> {
    runner_provider(label).map(|found| (found.provider.name, found.os))
}

#[test]
fn every_provider_has_a_source_and_a_retrieval_date() {
    for provider in RUNNER_PROVIDERS {
        assert!(provider.source.starts_with("https://"), "{}", provider.name);
        assert_eq!(
            provider.retrieved.len(),
            "2026-10-19".len(),
            "{}",
            provider.name
        );
    }
}

#[test]
fn blacksmith_labels_name_their_os() {
    let linux = Some(("Blacksmith", Some(RunnerOs::Linux)));
    assert_eq!(provider("blacksmith-4vcpu-ubuntu-2404"), linux);
    assert_eq!(provider("blacksmith-32vcpu-ubuntu-2204-arm"), linux);
    assert_eq!(
        provider("blacksmith-8vcpu-windows-2025"),
        Some(("Blacksmith", Some(RunnerOs::Windows)))
    );
    let macos = Some(("Blacksmith", Some(RunnerOs::MacOs)));
    assert_eq!(provider("blacksmith-6vcpu-macos-15"), macos);
    assert_eq!(provider("blacksmith-12vcpu-macos-latest"), macos);
    assert_eq!(provider("blacksmith-ubuntu-2404"), None);
    assert_eq!(provider("blacksmith-4vcpu-debian-12"), None);
}

#[test]
fn depot_labels_name_their_os() {
    let linux = Some(("Depot", Some(RunnerOs::Linux)));
    assert_eq!(provider("depot-ubuntu-24.04"), linux);
    assert_eq!(provider("depot-ubuntu-22.04-16"), linux);
    assert_eq!(provider("depot-ubuntu-24.04-arm"), linux);
    assert_eq!(provider("depot-ubuntu-24.04-arm-64"), linux);
    assert_eq!(provider("depot-ubuntu-latest"), linux);
    let windows = Some(("Depot", Some(RunnerOs::Windows)));
    assert_eq!(provider("depot-windows-2025"), windows);
    assert_eq!(provider("depot-windows-2022-8"), windows);
    assert_eq!(provider("depot-windows-latest"), windows);
    let macos = Some(("Depot", Some(RunnerOs::MacOs)));
    assert_eq!(provider("depot-macos-26"), macos);
    assert_eq!(provider("depot-macos-latest"), macos);
    assert_eq!(provider("depot-ubuntu-2404"), None);
}

#[test]
fn namespace_labels_name_their_os() {
    assert_eq!(
        provider("nscloud-ubuntu-24.04-amd64-4x8"),
        Some(("Namespace", Some(RunnerOs::Linux)))
    );
    assert_eq!(
        provider("nscloud-ubuntu-22.04-arm64-8x16-with-cache-with-features"),
        Some(("Namespace", Some(RunnerOs::Linux)))
    );
    assert_eq!(
        provider("nscloud-windows-2022-amd64-4x8"),
        Some(("Namespace", Some(RunnerOs::Windows)))
    );
    assert_eq!(
        provider("nscloud-macos-tahoe-slim-arm64-6x14"),
        Some(("Namespace", Some(RunnerOs::MacOs)))
    );
    assert_eq!(
        provider("namespace-profile-default"),
        Some(("Namespace", None))
    );
    assert_eq!(
        provider("namespace-features:container.privileged=true"),
        Some(("Namespace", None))
    );
    assert_eq!(provider("nscloud"), None);
}

#[test]
fn runs_on_key_value_labels_are_known() {
    assert_eq!(
        provider("runs-on=12345/runner=2cpu-linux-x64"),
        Some(("RunsOn", None))
    );
    assert_eq!(
        provider("runs-on=12345,cpu=2+8,family=c8a.large,image=ubuntu24-full-x64"),
        Some(("RunsOn", None))
    );
    assert_eq!(provider("image=ubuntu22-full-x64"), Some(("RunsOn", None)));
    assert_eq!(provider("Family=m8a+m8i-flex"), Some(("RunsOn", None)));
    assert_eq!(provider("color=blue"), None);
    assert_eq!(provider("runs-on=1/color=blue"), None);
    assert_eq!(provider("runner"), None);
}

#[test]
fn runs_on_labels_get_their_os_from_their_words() {
    assert_eq!(
        runner_label_os("runs-on=12345/runner=2cpu-linux-x64"),
        Some(RunnerOs::Linux)
    );
    assert_eq!(
        runner_label_os("runs-on=12345/image=windows25-full-x64"),
        Some(RunnerOs::Windows)
    );
    assert_eq!(runner_label_os("runs-on=12345/cpu=2"), None);
}

#[test]
fn ubicloud_labels_are_linux() {
    let linux = Some(("Ubicloud", Some(RunnerOs::Linux)));
    for label in [
        "ubicloud",
        "ubicloud-arm",
        "ubicloud-standard-2",
        "ubicloud-standard-30-arm",
        "ubicloud-standard-4-ubuntu-2204",
        "ubicloud-standard-4-arm-ubuntu-2604",
    ] {
        assert_eq!(provider(label), linux, "{label}");
    }
    assert_eq!(provider("ubicloud-premium-2"), None);
}

#[test]
fn warpbuild_labels_name_their_os() {
    let linux = Some(("WarpBuild", Some(RunnerOs::Linux)));
    assert_eq!(provider("warp-ubuntu-latest-x64-4x"), linux);
    assert_eq!(provider("warp-ubuntu-2204-x64-32x"), linux);
    assert_eq!(provider("warp-ubuntu-latest-arm64-2x"), linux);
    assert_eq!(provider("warp-ubuntu-2604-arm64-6x-xfast"), linux);
    let windows = Some(("WarpBuild", Some(RunnerOs::Windows)));
    assert_eq!(provider("warp-windows-latest-x64-4x"), windows);
    assert_eq!(provider("warp-windows-2025-x64-16x"), windows);
    assert_eq!(provider("warp-windows-2025-vs2026-x64-8x"), windows);
    let macos = Some(("WarpBuild", Some(RunnerOs::MacOs)));
    assert_eq!(provider("warp-macos-latest-arm64-6x"), macos);
    assert_eq!(provider("warp-macos-15-arm64-12x"), macos);
    assert_eq!(provider("warp-ubuntu-latest-x64"), None);
}

#[test]
fn provider_labels_compare_without_case() {
    assert_eq!(
        provider("Depot-Ubuntu-24.04"),
        Some(("Depot", Some(RunnerOs::Linux)))
    );
}

#[test]
fn custom_label_that_names_windows_is_windows() {
    assert_eq!(
        runner_label_os("self-32vcpu-windows-2022"),
        Some(RunnerOs::Windows)
    );
    assert_eq!(runner_label_os("build-Windows"), Some(RunnerOs::Windows));
    assert_eq!(runner_label_os("gpu-ubuntu22.04"), Some(RunnerOs::Linux));
    assert_eq!(runner_label_os("mac-macos14-arm64"), Some(RunnerOs::MacOs));
}

#[test]
fn custom_label_without_an_os_has_none() {
    assert_eq!(runner_label_os("self-hosted"), None);
    assert_eq!(runner_label_os("gpu-runner"), None);
    assert_eq!(runner_label_os("windowsill"), None);
    assert_eq!(runner_label_os("linux-to-windows"), None);
}

#[test]
fn reference_prefixes_decide_the_os_first() {
    assert_eq!(runner_label_os("windows-latest"), Some(RunnerOs::Windows));
    assert_eq!(runner_label_os("windows-on-linux"), Some(RunnerOs::Windows));
    assert_eq!(runner_label_os("ubuntu-on-windows"), Some(RunnerOs::Linux));
    assert_eq!(runner_label_os("macos"), Some(RunnerOs::MacOs));
    assert_eq!(runner_label_os("LINUX"), Some(RunnerOs::Linux));
}

#[test]
fn runs_on_resolves_to_hosted_self_hosted_or_unknown() {
    let hosted = resolve_runner([Some("ubuntu-latest")]);
    assert_eq!(hosted.kind, RunnerKind::Hosted { provider: None });
    assert_eq!(hosted.os, Some(RunnerOs::Linux));

    let provider = resolve_runner([Some("depot-windows-2022")]);
    assert!(
        matches!(provider.kind, RunnerKind::Hosted { provider: Some(p) } if p.name == "Depot"),
        "{:?}",
        provider.kind
    );
    assert_eq!(provider.os, Some(RunnerOs::Windows));

    let self_hosted = resolve_runner([Some("self-hosted"), Some("self-32vcpu-windows-2022")]);
    assert_eq!(self_hosted.kind, RunnerKind::SelfHosted);
    assert_eq!(self_hosted.os, Some(RunnerOs::Windows));

    let preset = resolve_runner([Some("linux"), Some("arm64")]);
    assert_eq!(preset.kind, RunnerKind::SelfHosted);
    assert_eq!(preset.os, Some(RunnerOs::Linux));

    let custom = resolve_runner([Some("gpu-runner")]);
    assert_eq!(custom.kind, RunnerKind::Unknown);
    assert_eq!(custom.os, None);

    let expression = resolve_runner([None]);
    assert_eq!(expression.kind, RunnerKind::Unknown);
    assert_eq!(expression.os, None);
}

#[test]
fn labels_that_name_two_systems_have_no_os() {
    assert_eq!(
        resolve_runner([Some("ubuntu-latest"), Some("windows-latest")]).os,
        None
    );
    assert_eq!(
        resolve_runner([Some("self-hosted"), Some("linux"), Some("gpu")]).os,
        Some(RunnerOs::Linux)
    );
}
