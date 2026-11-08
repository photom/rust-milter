# The runner builds the load tests in release mode while the build still has
# network access, then runs them with no network (testing ADR 0005).
FROM rust:1.99-slim-trixie
WORKDIR /src
COPY . .
RUN cargo test -p rust-milter-load --release --locked --no-run
ENV CARGO_NET_OFFLINE=true
CMD ["cargo", "test", "-p", "rust-milter-load", "--release", "--locked", "--", "--ignored", "--test-threads=1", "--nocapture"]
