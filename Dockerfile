FROM public.ecr.aws/r3m4q3r9/qovery-ci:rust-1.80.1-2024-10-21T15-59-17 as build

RUN apt-key del 234654DA9A296436 || true
RUN curl -fsSL https://pkgs.k8s.io/core:/stable:/v1.30/deb/Release.key | gpg --dearmor -o /usr/share/keyrings/kubernetes-archive-keyring.gpg
RUN echo "deb [signed-by=/usr/share/keyrings/kubernetes-archive-keyring.gpg] https://pkgs.k8s.io/core:/stable:/v1.30/deb /" | tee /etc/apt/sources.list.d/kubernetes.list

RUN apt-get update && \
  apt-get install -y librust-openssl-sys-dev curl && \
  apt-get clean && \
  rm -rf /var/lib/apt/lists && \
  mkdir -p /build

WORKDIR /build
ADD . /build
RUN cargo build --release

FROM debian:12-slim as run

ENV RUST_LOG=info
RUN apt-get update && apt-get -y dist-upgrade && apt-get install -y ca-certificates && apt-get clean
COPY --from=build /build/target/release/iam-eks-user-mapper /usr/bin/iam-eks-user-mapper
RUN chmod 755 /usr/bin/iam-eks-user-mapper

CMD ["/usr/bin/iam-eks-user-mapper"]
