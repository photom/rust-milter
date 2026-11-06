#!/bin/sh
# Sendmail settings for the interoperability stack. The test domain
# void.invalid is relayed to the SMTP sink in the runner.
set -eu
mc=/etc/mail/sendmail.mc

filter="S=inet:8891@runner.void.invalid, F=T"
if [ -n "${FILTER_TIMEOUTS:-}" ]; then
  filter="$filter, T=$FILTER_TIMEOUTS"
fi

# Listen on every interface of the container, not only on loopback.
sed -i "s/, *Addr=127\.0\.0\.1//" "$mc"

# Insert the test settings before the mailer definitions.
sed -i "/^MAILER_DEFINITIONS/i\\
define(\`confDOMAIN_NAME', \`sendmail.void.invalid')dnl\\
FEATURE(\`accept_unresolvable_domains')dnl\\
FEATURE(\`mailertable', \`hash -o /etc/mail/mailertable.db')dnl\\
define(\`RELAY_MAILER_ARGS', \`TCP \$h 2525')dnl\\
INPUT_MAIL_FILTER(\`rust-milter', \`$filter')dnl" "$mc"

echo "void.invalid relay:[runner.void.invalid]" > /etc/mail/mailertable
makemap hash /etc/mail/mailertable < /etc/mail/mailertable
echo "void.invalid" > /etc/mail/relay-domains

# Debian's access database lets a client other than localhost open 10
# connections per 10 minutes, 10 at a time, and makes it wait 5 s for the
# greeting. The tests open many more sessions than that from the runner,
# so the stack's subnet gets the limits Debian gives localhost.
access=/etc/mail/access
if [ ! -f "$access" ]; then
  cp /usr/share/sendmail/examples/db/access "$access"
fi
cat >> "$access" <<'EOF'
ClientRate:10.231.0	0
ClientConn:10.231.0	0
GreetPause:10.231.0	0
EOF
makemap hash "$access" < "$access"

# Debian's sendmail.mc includes cf.m4 itself; include it only if it does not.
if grep -q "cf/m4/cf.m4" "$mc"; then
  m4 "$mc" > /etc/mail/sendmail.cf
else
  m4 /usr/share/sendmail/cf/m4/cf.m4 "$mc" > /etc/mail/sendmail.cf
fi
grep -q "^Xrust-milter" /etc/mail/sendmail.cf
