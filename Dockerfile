FROM public.ecr.aws/r3m4q3r9/qovery-ci:rust-1.80.1-2024-10-21T15-59-17 as build

RUN apt-get update && \
  apt-get install -y librust-openssl-sys-dev && \
  apt-get clean && \
  mkdir /build

WORKDIR /build
ADD . /build
RUN cargo build --release

FROM debian:12-slim as run

ENV RUST_LOG=info
RUN apt-get update && apt-get -y dist-upgrade && apt-get install -y ca-certificates && apt-get clean
COPY --from=build /build/target/release/iam-eks-user-mapper /usr/bin/iam-eks-user-mapper
RUN chmod 755 /usr/bin/iam-eks-user-mapper

CMD ["/usr/bin/iam-eks-user-mapper"]
