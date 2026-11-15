FROM debian:13-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends unbound \
 && rm -rf /var/lib/apt/lists/*
COPY unbound.conf /etc/unbound/unbound.conf
RUN unbound-checkconf /etc/unbound/unbound.conf
EXPOSE 53/udp 53/tcp
# The container resolves through its own Unbound, so that the healthcheck in
# compose.yaml can ask it with getent.
CMD ["sh", "-c", "echo \"nameserver 127.0.0.1\" > /etc/resolv.conf && unbound -V | head -n 1 && exec unbound -d -c /etc/unbound/unbound.conf"]
