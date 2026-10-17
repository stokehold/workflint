//! Checks the branding colors and icons, the Docker registry prefixes, and the `runs.using`
//! values against the reference's `BrandingColors`, `BrandingIcons`,
//! `isImageOnDockerRegistry`, and `checkLocalActionRuns` in the vendored `rule_action.go`.

mod go_source;

use go_source::{
    branding_colors, branding_icons, docker_registry_prefixes, invalid_runs_using_message,
    runs_using_values,
};
use workflint_data::{
    BRANDING_COLORS, BRANDING_ICONS, DOCKER_REGISTRY_PREFIXES, RUNS_USING_VALUES,
    is_branding_color, is_branding_icon, is_on_docker_registry, is_runs_using_value,
};

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

#[test]
fn branding_colors_match_the_reference_table() {
    assert_eq!(BRANDING_COLORS, sorted(branding_colors()));
}

#[test]
fn branding_icons_match_the_reference_table() {
    assert_eq!(BRANDING_ICONS, sorted(branding_icons()));
}

#[test]
fn branding_lookups_lower_the_case_as_go_does() {
    assert!(is_branding_color("Blue"));
    assert!(is_branding_icon("ZAP"));
    // Go's `strings.ToLower` maps the Kelvin sign to `k` and `İ` to `i`.
    assert!(is_branding_icon("LOC\u{212a}"));
    assert!(is_branding_icon("L\u{130}NK"));
    assert!(!is_branding_icon("zap "));
    assert!(!is_branding_color("pink"));
}

#[test]
fn docker_registry_prefixes_match_the_reference_function() {
    assert_eq!(DOCKER_REGISTRY_PREFIXES, docker_registry_prefixes());
}

#[test]
fn docker_registry_lookup_is_a_case_sensitive_prefix_match() {
    assert!(is_on_docker_registry("ghcr.io/owner/image:1"));
    assert!(is_on_docker_registry("docker://alpine"));
    assert!(!is_on_docker_registry("GHCR.IO/owner/image"));
    assert!(!is_on_docker_registry("Dockerfile"));
}

#[test]
fn runs_using_values_match_the_reference_switch() {
    assert_eq!(RUNS_USING_VALUES, sorted(runs_using_values()));
}

/// The message lists the values, so it must change with them.
#[test]
fn runs_using_values_match_the_reference_message() {
    let listed = match RUNS_USING_VALUES {
        [init @ .., last] => {
            let init: Vec<String> = init.iter().map(|value| format!("{value:?}")).collect();
            format!("{}, and {last:?}", init.join(", "))
        }
        [] => String::new(),
    };
    let message = invalid_runs_using_message();
    assert!(
        message.contains(&format!("valid runners are {listed}.")),
        "{message}"
    );
}

#[test]
fn runs_using_lookup_is_case_sensitive() {
    assert!(is_runs_using_value("node24"));
    assert!(!is_runs_using_value("Node24"));
    assert!(!is_runs_using_value("node16"));
    assert!(!is_runs_using_value(""));
}
