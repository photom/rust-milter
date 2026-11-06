#!/bin/sh
# Postfix settings for the interoperability stack. The test domain
# void.invalid is relayed to the SMTP sink in the runner.
set -eu

postconf -e \
  'compatibility_level = 3.6' \
  'myhostname = postfix.void.invalid' \
  'mydestination =' \
  'inet_protocols = ipv4' \
  'relay_domains = void.invalid' \
  'transport_maps = inline:{ void.invalid=smtp:[runner.void.invalid]:2525 }' \
  'smtpd_milters = inet:runner.void.invalid:8891' \
  'milter_protocol = 6' \
  'milter_default_action = tempfail' \
  'maillog_file = /dev/stdout'

# Run every service outside the chroot, so name lookups use the
# container's resolver.
postconf -F '*/*/chroot = n'

# Extra smtpd listeners, each overriding one setting for a scenario.
postconf -M \
  '2602/inet=2602 inet n - n - - smtpd -o milter_protocol=2' \
  '2603/inet=2603 inet n - n - - smtpd -o milter_protocol=3' \
  '2604/inet=2604 inet n - n - - smtpd -o milter_protocol=4' \
  '2625/inet=2625 inet n - n - - smtpd -o milter_content_timeout=2s' \
  '2626/inet=2626 inet n - n - - smtpd -o smtpd_peername_lookup=no'
