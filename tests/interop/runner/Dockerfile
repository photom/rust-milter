# The runner builds the interop tests while the build still has network
# access, then runs them offline inside the internal Compose network.
FROM rust:1.99-slim-trixie@sha256:f6c4a35d21dbb3c89d7a8d056c193e94fc09037f2b58dd858b09c39a47dd02d7
WORKDIR /src
COPY . .
RUN cargo test -p rust-milter-interop --locked --no-run
ENV CARGO_NET_OFFLINE=true
CMD ["cargo", "test", "-p", "rust-milter-interop", "--locked", "--", "--ignored", "--test-threads=1"]
