//! Checks the time zones against the list that `cargo xtask vendor-testdata` takes from the
//! tzdata of the Go that builds the reference, and that the reference embeds that tzdata.

mod go_source;

use go_source::{MAIN_GO, go_time_zones};
use workflint_data::{TIME_ZONES, TZDATA_VERSION, is_time_zone};

#[test]
fn time_zones_match_the_go_tzdata() {
    let (_, names) = go_time_zones();
    assert_eq!(TIME_ZONES, names);
}

#[test]
fn tzdata_version_matches_the_go_tzdata() {
    let (version, _) = go_time_zones();
    assert_eq!(TZDATA_VERSION, version);
}

/// Without the embedded tzdata, the reference's answer would come only from the host.
#[test]
fn reference_binary_embeds_the_go_tzdata() {
    assert!(MAIN_GO.contains("_ \"time/tzdata\""), "{MAIN_GO}");
}

#[test]
fn time_zones_are_sorted_for_binary_search() {
    assert!(TIME_ZONES.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn lookup_is_an_exact_match() {
    assert!(is_time_zone("America/New_York"));
    assert!(is_time_zone("US/Pacific"));
    assert!(is_time_zone("UTC"));
    assert!(!is_time_zone("america/new_york"));
    assert!(!is_time_zone("Not/A/Timezone"));
    assert!(!is_time_zone("posixrules"));
    assert!(!is_time_zone(""));
}
