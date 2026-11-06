FROM debian:13-slim
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      sendmail sendmail-cf m4 make rsyslog \
 && rm -rf /var/lib/apt/lists/*
# Filter timeouts for the milter (Sendmail's T= equate), empty for the defaults.
ARG FILTER_TIMEOUTS=""
COPY configure.sh /usr/local/sbin/configure-sendmail
COPY rsyslog.conf /etc/rsyslog.conf
RUN FILTER_TIMEOUTS="$FILTER_TIMEOUTS" sh /usr/local/sbin/configure-sendmail
EXPOSE 25
# The stack's Unbound, at $RESOLVER, is the only resolver (see compose.yaml).
# Sendmail logs through syslog. rsyslogd writes the log to its stdout (see
# rsyslog.conf), and runs in the foreground (-n), in the background of the
# shell, so that its stdout stays the container's. busybox syslogd did not
# work here: it dropped even a message from logger. The logger line shows
# that the log works before Sendmail starts.
CMD ["sh", "-c", "echo \"nameserver $RESOLVER\" > /etc/resolv.conf && { rsyslogd -n -f /etc/rsyslog.conf & } && sleep 1 && { logger -p mail.info 'syslog check: rsyslogd is running' || true; } && sendmail -d0.1 -bt < /dev/null | head -n 1 && exec sendmail -bD -q30m"]
