# The runner builds the load tests in release mode while the build still has
# network access, then runs them with no network (testing ADR 0006).
FROM rust:1.99-slim-trixie@sha256:f6c4a35d21dbb3c89d7a8d056c193e94fc09037f2b58dd858b09c39a47dd02d7
WORKDIR /src
COPY . .
RUN cargo test -p rust-milter-load --release --locked --no-run
ENV CARGO_NET_OFFLINE=true
CMD ["cargo", "test", "-p", "rust-milter-load", "--release", "--locked", "--", "--ignored", "--test-threads=1", "--nocapture"]
