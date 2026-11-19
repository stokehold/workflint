# Not built. The docker-build job of .github/workflows/release.yml reads these
# two image references, so that Dependabot updates their pins with the base
# images of docker/Dockerfile.

# The BuildKit of the docker-container builder.
FROM moby/buildkit:v0.35.0@sha256:aa632b921ef07411d2cb77292cd34213d4f7b384001d6fceef1e1f25bf9752bb AS buildkit

# The scanner that writes the SBOM attestations.
FROM docker/buildkit-syft-scanner:1.12.1@sha256:873a7458c3c8ac8e32c7b796c10e1600b07b963d2aa000a72a2d97aaaa2d6315 AS sbom-scanner
