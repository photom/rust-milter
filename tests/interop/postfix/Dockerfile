FROM debian:13-slim
RUN echo "postfix postfix/main_mailer_type select Internet Site" | debconf-set-selections \
 && echo "postfix postfix/mailname string postfix.void.invalid" | debconf-set-selections \
 && apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends postfix \
 && rm -rf /var/lib/apt/lists/*
COPY configure.sh /usr/local/sbin/configure-postfix
RUN sh /usr/local/sbin/configure-postfix
EXPOSE 25 2602 2603 2604 2625 2626
# The stack's Unbound, at $RESOLVER, is the only resolver (see compose.yaml).
CMD ["sh", "-c", "echo \"nameserver $RESOLVER\" > /etc/resolv.conf && echo \"postfix version: $(postconf -h mail_version)\" && exec postfix start-fg"]
