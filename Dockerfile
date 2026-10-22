FROM public.ecr.aws/r3m4q3r9/qovery-ci:rust-1.98.1-2026-10-15T13-47-13 as build

WORKDIR /build
ADD . /build
RUN cargo build --release

FROM debian:13-slim as run

ENV RUST_LOG=info
RUN apt-get update && apt-get -y dist-upgrade && apt-get install -y ca-certificates && apt-get clean
COPY --from=build /build/target/release/iam-eks-user-mapper /usr/bin/iam-eks-user-mapper
RUN chmod 755 /usr/bin/iam-eks-user-mapper

CMD ["/usr/bin/iam-eks-user-mapper"]
