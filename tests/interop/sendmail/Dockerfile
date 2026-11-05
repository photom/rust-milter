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
# Sendmail logs through syslog; busybox syslogd forwards the log to stdout.
# syslogd runs in the foreground (-n) in the background of the shell: as a
# daemon it would point its stdout at /dev/null before opening the log.
CMD ["sh", "-c", "syslogd -n -O /dev/stdout & sendmail -d0.1 -bt < /dev/null | head -n 1 && exec sendmail -bD -q30m"]
