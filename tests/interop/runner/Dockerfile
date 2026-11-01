# The runner builds the interop tests while the build still has network
# access, then runs them offline inside the internal Compose network.
FROM rust:1.99-slim-trixie
WORKDIR /src
COPY . .
RUN cargo test -p rust-milter-interop --locked --no-run
ENV CARGO_NET_OFFLINE=true
CMD ["cargo", "test", "-p", "rust-milter-interop", "--locked", "--", "--ignored", "--test-threads=1"]
