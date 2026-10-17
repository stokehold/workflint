//! The prefixes that mark the image of a Docker action as one to pull from a registry instead
//! of a `Dockerfile` to build.
//!
//! Ported from rhysd/actionlint at commit `011a6d1`: `isImageOnDockerRegistry` in
//! `rule_action.go`, lines 291-297. Until M7 replaces it with a generated file (ADR 0011), keep
//! it equal to that function. The test `docker_registry_prefixes_match_the_reference_function`
//! compares them.
//!
//! <https://docs.github.com/en/actions/creating-actions/metadata-syntax-for-github-actions#runsimage>

/// The prefixes, in the reference's order.
pub static DOCKER_REGISTRY_PREFIXES: &[&str] =
    &["docker://", "gcr.io/", "pkg.dev/", "ghcr.io/", "docker.io/"];

/// Returns whether `image` starts with one of [`DOCKER_REGISTRY_PREFIXES`]. The comparison is
/// case-sensitive.
#[must_use]
pub fn is_on_docker_registry(image: &str) -> bool {
    DOCKER_REGISTRY_PREFIXES
        .iter()
        .any(|prefix| image.starts_with(prefix))
}
