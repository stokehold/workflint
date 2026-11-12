# Not built. The docker-build job of .github/workflows/release.yml reads these
# two image references, so that Dependabot updates their pins with the base
# images of docker/Dockerfile.

# The BuildKit of the docker-container builder.
FROM moby/buildkit:v0.34.2@sha256:c5f8862f52499773af85bc07695b2379ef08e8002352b0df2c4864c91d78dabe AS buildkit

# The scanner that writes the SBOM attestations.
FROM docker/buildkit-syft-scanner:1.12.1@sha256:873a7458c3c8ac8e32c7b796c10e1600b07b963d2aa000a72a2d97aaaa2d6315 AS sbom-scanner
