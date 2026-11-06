# Testing ADR 0003: The interoperability stack resolves names with a local Unbound

- Status: Accepted
- Date: 2026-11-05
- Affects: test environment, CI

## Context

The Compose network of the interoperability stack is internal: no container can reach the internet, so test mail cannot leave the stack ([testing ADR 0001](0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md)). Docker's resolver answers for the service names, and nothing answers for any other name.

The MTAs look names up all the time: the milter's address, their own host name, the recipient's domain, the client's address. In the first CI runs:
- Sendmail queued every message instead of delivering it, because its lookup of `void.invalid` failed with a temporary error.
- Sendmail resolves the milter's host when it reads its configuration. The runner, which holds the milter, starts after the MTAs, so Docker's resolver did not know its name yet. Sendmail retried for about 87 seconds before it answered its first connection. Once its other lookups were made fast, it reached this one before the runner existed, and exited.

Each failure could be worked around one at a time with a Sendmail option such as `FEATURE(nocanonify)`. Every workaround moves the MTA further from how it runs in production, and the next lookup could fail the same way.

## Options considered

- (a) Work around each lookup in the MTA configuration: `nocanonify`, `DONT_PROBE_INTERFACES`, fixed host names, and Postfix without DNS.
- (b) A local DNS server in the stack that answers for the names in `void.invalid`, and answers NXDOMAIN at once for everything else.
- (c) Give the network a route to a public resolver.

## Decision

Option (b). A `dns` service runs Unbound from the pinned Debian release. It serves A and PTR records for every container's name in `void.invalid`, and an MX record for `void.invalid`. Every other name gets NXDOMAIN without a query leaving the stack. The services have fixed addresses on a fixed subnet, so the records can name them. Each MTA uses `dns` as its only resolver, starts only once `dns` answers, and names the runner as `runner.void.invalid`. The runner's name then resolves before the runner exists.

Option (c) was rejected because it breaks the rule that test mail cannot leave the stack.

## Consequences

Positive:
- The MTAs resolve names as they do in production, with their default lookups on. Every lookup gets a definite answer at once, so no lookup can stall a test.
- One file, `tests/interop/dns/unbound.conf`, lists every name and address in the stack.
- Unbound logs every query, so a failing test shows what an MTA looked up.

Negative:
- The stack has one more image to build and keep up to date.
- The addresses are fixed. A new service needs an address and records in both `compose.yaml` and `unbound.conf`, and the subnet must not overlap a network on the machine that runs the stack.
- Mail to `void.invalid` still goes through `transport_maps` and the mailertable, because the sink listens on port 2525 and an MX record cannot name a port.

## References

- [test-environment.md, Interoperability environment](../../reference/test-environment.md#interoperability-environment)
- [Testing ADR 0001](0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md)
