# Not built. The docker-build job of .github/workflows/release.yml reads these
# two image references, so that Dependabot updates their pins with the base
# images of docker/Dockerfile.

# The BuildKit of the docker-container builder.
FROM moby/buildkit:v0.34.1@sha256:99368f3e46177c6405b9740e4461d91cee004128daeecca78d4ceac50ec1da5c AS buildkit

# The scanner that writes the SBOM attestations.
FROM docker/buildkit-syft-scanner:1.12.0@sha256:ae4f3b554449e7e25548e7d8ccc029d17357348e30c6e3df01b92bc93654d6a9 AS sbom-scanner
