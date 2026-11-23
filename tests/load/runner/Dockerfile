# The runner builds the load tests in release mode while the build still has
# network access, then runs them with no network (testing ADR 0006).
FROM rust:1.100-slim-trixie@sha256:6f14b2b4b278ec0d50aa1d69541c304ce0f95e0f6268487aa9924af1c2e9aebe
WORKDIR /src
COPY . .
RUN cargo test -p rust-milter-load --release --locked --no-run
ENV CARGO_NET_OFFLINE=true
CMD ["cargo", "test", "-p", "rust-milter-load", "--release", "--locked", "--", "--ignored", "--test-threads=1", "--nocapture"]
