FROM debian:13-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      sendmail sendmail-cf m4 make busybox-syslogd \
 && rm -rf /var/lib/apt/lists/*
# Filter timeouts for the milter (Sendmail's T= equate), empty for the defaults.
ARG FILTER_TIMEOUTS=""
COPY configure.sh /usr/local/sbin/configure-sendmail
RUN FILTER_TIMEOUTS="$FILTER_TIMEOUTS" sh /usr/local/sbin/configure-sendmail
EXPOSE 25
# The stack's Unbound, at $RESOLVER, is the only resolver (see compose.yaml).
# Sendmail logs through syslog; busybox syslogd writes the log to its stdout
# (-O -). It runs in the foreground (-n), in the background of the shell, so
# that its stdout stays the container's. The logger line checks that syslogd
# works at all, because Sendmail's log has not appeared so far.
CMD ["sh", "-c", "echo \"nameserver $RESOLVER\" > /etc/resolv.conf && { syslogd -n -O - & } && sleep 1 && { logger -p mail.info 'syslog check: syslogd is running' || true; } && sendmail -d0.1 -bt < /dev/null | head -n 1 && exec sendmail -bD -q30m"]
