# Embeds the crate dependency tree in the release binary so image scanners
# (Amazon Inspector, trivy, grype, `cargo audit bin`) can audit it like any other artifact.
# https://github.com/rust-secure-code/cargo-auditable/releases
ARG CARGO_AUDITABLE_VERSION=0.7.7

FROM public.ecr.aws/r3m4q3r9/qovery-ci:rust-1.98.1-2026-10-29T11-57-17 as build

ARG CARGO_AUDITABLE_VERSION

# Prebuilt static cargo-auditable, verified against the checksum published with the release.
RUN <<EOF
  set -eu
  arch=$(dpkg --print-architecture | sed 's/amd64/x86_64/; s/arm64/aarch64/')
  asset="cargo-auditable-${arch}-unknown-linux-musl.tar.xz"
  base="https://github.com/rust-secure-code/cargo-auditable/releases/download/v${CARGO_AUDITABLE_VERSION}"
  tmp=$(mktemp -d)
  cd "$tmp"
  curl -fsSLO "${base}/${asset}"
  curl -fsSLO "${base}/${asset}.sha256"
  sha256sum -c "${asset}.sha256"
  tar -xJf "$asset" --strip-components=1 -C /usr/local/bin "${asset%.tar.xz}/cargo-auditable"
  cd /
  rm -rf "$tmp"
  cargo auditable --version
EOF

WORKDIR /build
ADD . /build
RUN <<EOF
  set -eu
  cargo auditable build --release

  # cargo-auditable must have embedded the dependency tree in the binary the run stage ships.
  readelf -S /build/target/release/iam-eks-user-mapper | grep -qF ".dep-v0" || { echo "missing .dep-v0 section in iam-eks-user-mapper" >&2; exit 1; }
EOF

FROM debian:13-slim as run

ENV RUST_LOG=info
RUN apt-get update && apt-get -y dist-upgrade && apt-get install -y ca-certificates && apt-get clean
COPY --from=build /build/target/release/iam-eks-user-mapper /usr/bin/iam-eks-user-mapper
RUN chmod 755 /usr/bin/iam-eks-user-mapper

CMD ["/usr/bin/iam-eks-user-mapper"]
