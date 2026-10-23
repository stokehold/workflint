# The images of ghcr.io/stokehold/workflint (docs/releasing.md). Both final
# stages copy the release binary for TARGETARCH from the build context `dist`,
# which holds linux_amd64/workflint and linux_arm64/workflint, and compile
# nothing:
#
#   docker buildx build docker --build-context dist=<directory> \
#     --build-arg VERSION=<version> --target default
#
# default  workflint alone, on distroless static.
# full     workflint with shellcheck, ruff, and pyflakes, on Python on Alpine.
#
# Every base image is pinned by digest, and Dependabot updates the pins.

# shellcheck is GPL-3.0 and ships as its own executable (ADR 0017). Each
# stage fetches the release archive of one architecture and checks it against
# the digest that GitHub shows for the asset.
FROM scratch AS shellcheck-amd64
ADD --checksum=sha256:b7af85e41cc99489dcc21d66c6d5f3685138f06d34651e6d34b42ec6d54fe6f6 \
  https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.x86_64.tar.gz \
  /shellcheck.tar.gz

FROM scratch AS shellcheck-arm64
ADD --checksum=sha256:68a8133197a50beb8803f8d42f9908d1af1c5540d4bb05fdfca8c1fa47decefc \
  https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.aarch64.tar.gz \
  /shellcheck.tar.gz

FROM shellcheck-${TARGETARCH} AS shellcheck

FROM gcr.io/distroless/static-debian13:nonroot@sha256:d62067879de890a1def9da0e7766b7d20810a5e5555cef8506b752437d7a3e88 AS default
ARG TARGETARCH
ARG VERSION
LABEL org.opencontainers.image.source=https://github.com/stokehold/workflint \
  org.opencontainers.image.licenses=MIT \
  org.opencontainers.image.version=$VERSION
ENV WORKFLINT_INSTALLED_FROM="installed from Docker image"
COPY --from=dist --chmod=0755 linux_${TARGETARCH}/workflint /usr/local/bin/workflint
# The nonroot user of the base image.
USER 65532:65532
WORKDIR /repo
ENTRYPOINT ["/usr/local/bin/workflint"]

FROM python:3.14-alpine3.24@sha256:601e99e090cb08c2784a42395487709500bc39930307fe72f3f11a8f3c430762 AS full
ARG TARGETARCH
ARG VERSION
LABEL org.opencontainers.image.source=https://github.com/stokehold/workflint \
  org.opencontainers.image.licenses=MIT \
  org.opencontainers.image.version=$VERSION
ENV WORKFLINT_INSTALLED_FROM="installed from Docker image"
# pip installs only the pinned wheels with their hashes. The user has the uid
# of distroless's nonroot user, so both images run as the same user.
RUN --mount=from=shellcheck,target=/mnt/shellcheck \
  --mount=type=bind,source=requirements-full.txt,target=/mnt/requirements-full.txt \
  set -eu; \
  mkdir /tmp/shellcheck; \
  tar -xzf /mnt/shellcheck/shellcheck.tar.gz -C /tmp/shellcheck --strip-components 1; \
  install -m 0755 /tmp/shellcheck/shellcheck /usr/local/bin/shellcheck; \
  install -d /usr/share/doc/shellcheck; \
  install -m 0644 /tmp/shellcheck/LICENSE.txt /usr/share/doc/shellcheck/LICENSE.txt; \
  rm -r /tmp/shellcheck; \
  pip install --no-cache-dir --no-deps --only-binary :all: --require-hashes \
    --disable-pip-version-check --root-user-action ignore -r /mnt/requirements-full.txt; \
  adduser -D -u 65532 nonroot
COPY --chmod=0644 shellcheck-source.txt /usr/share/doc/shellcheck/SOURCE.txt
COPY --from=dist --chmod=0755 linux_${TARGETARCH}/workflint /usr/local/bin/workflint
USER 65532:65532
WORKDIR /repo
ENTRYPOINT ["/usr/local/bin/workflint"]
