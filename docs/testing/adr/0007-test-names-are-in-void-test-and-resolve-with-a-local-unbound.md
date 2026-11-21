# Testing ADR 0007: Test names are in `void.test`, and the interoperability stack resolves them with a local Unbound

- Status: Accepted
- Date: 2026-11-21
- Supersedes: [testing ADR 0003](0003-interop-stack-resolves-names-with-a-local-unbound.md), which put the test names in `void.invalid`, a domain that RFC 6761 does not let a DNS server answer for
- Affects: test environment, CI, tests, test scenarios, examples in the documentation

## Context

Tests, test scenarios, and examples in the documentation use email addresses and host names. None of them may name a real domain, so that test mail cannot reach anyone.

The Compose network of the interoperability stack is internal: no container can reach the internet, so test mail cannot leave the stack ([testing ADR 0001](0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md)). Docker's resolver answers for the service names, and nothing answers for any other name.

The MTAs look names up all the time: the milter's address, their own host name, the recipient's domain, the client's address. In the first CI runs:
- Sendmail queued every message instead of delivering it, because its lookup of `void.invalid` failed with a temporary error.
- Sendmail resolves the milter's host when it reads its configuration. The runner, which holds the milter, starts after the MTAs, so Docker's resolver did not know its name yet. Sendmail retried for about 87 seconds before it answered its first connection. Once its other lookups were made fast, it reached this one before the runner existed, and exited.

Each failure could be worked around one at a time with a Sendmail option such as `FEATURE(nocanonify)`. Every workaround moves the MTA further from how it runs in production, and the next lookup could fail the same way.

Testing ADR 0003 put the test names in `void.invalid`. RFC 2606 §2 reserves four top-level domains: `.test` "for use in testing of current or new DNS related code", `.example` for documentation, `.invalid` for names "that are sure to be invalid", and `.localhost`. Its §3 adds `example.com`, `example.net`, and `example.org` as examples. Under RFC 6761, resolvers and DNS servers treat these names as follows:

| Domain | Resolver libraries | DNS servers | RFC 6761 |
|---|---|---|---|
| `.invalid` | SHOULD answer with an immediate negative response, and SHOULD NOT send the query to a DNS server | SHOULD answer NXDOMAIN. The data "cannot be modified by local configuration" | §6.4 items 3 to 6 |
| `.test` | SHOULD NOT treat the names as special, and SHOULD send the query to the configured DNS server | SHOULD answer negatively "unless explicitly configured by the administrator to give positive answers for test names". An operator who uses test names SHOULD make its servers authoritative for them | §6.2 items 3 to 6 |
| `example.com` and the other example names | SHOULD NOT treat the names as special | SHOULD resolve them normally. The names are "reserved for use in documentation" and registered to IANA | §6.5 items 1, 3, 4, and 7 |

The stack's DNS server gave positive answers for `void.invalid`, against §6.4 items 5 and 6, and a resolver library that follows §6.4 item 3 would fail every lookup of a name in `void.invalid`.

## Options considered

Name resolution in the stack:
- (a) Work around each lookup in the MTA configuration: `nocanonify`, `DONT_PROBE_INTERFACES`, fixed host names, and Postfix without DNS.
- (b) A local DNS server in the stack that answers for the names in the test domain, and answers NXDOMAIN at once for everything else.
- (c) Give the network a route to a public resolver.

Domain for the test names:
- (d) Keep `void.invalid`.
- (e) Names under `example.com`.
- (f) Names under `void.test`.

## Decision

Options (b) and (f). Every test name is in `void.test`: addresses such as `sender@void.test` in tests, scenarios, and examples in the documentation, and host names such as `runner.void.test` in the stack.

A `dns` service runs Unbound from the pinned Debian release. It serves A and PTR records for every container's name in `void.test`, and an MX record for `void.test`, from its own data: RFC 6761 §6.2 items 5 and 6 let an administrator configure a DNS server to answer for test names. Every other name gets NXDOMAIN without a query leaving the stack. The services have fixed addresses on a fixed subnet, so the records can name them. Each MTA uses `dns` as its only resolver, starts only once `dns` answers, and names the runner as `runner.void.test`. The runner's name then resolves before the runner exists.

Option (c) was rejected because it breaks the rule that test mail cannot leave the stack. Option (d) was rejected because option (b) needs `dns` to answer for the test names, and RFC 6761 §6.4 item 6 says the data for `.invalid` names "cannot be modified by local configuration". Option (e) was rejected because RFC 6761 §6.5 reserves the example names for documentation and asks DNS servers to resolve them normally, so a lookup made outside the stack goes to the public DNS.

## Consequences

Positive:
- The MTAs resolve names as they do in production, with their default lookups on. Every lookup gets a definite answer at once, so no lookup can stall a test.
- Resolver libraries that follow RFC 6761 §6.2 item 3 send lookups of test names to `dns`, and `dns` answers them as §6.2 item 5 allows.
- One file, `tests/interop/dns/unbound.conf`, lists every name and address in the stack.
- Unbound logs every query, so a failing test shows what an MTA looked up.

Negative:
- The stack has one more image to build and keep up to date.
- The addresses are fixed. A new service needs an address and records in both `compose.yaml` and `unbound.conf`, and the subnet must not overlap a network on the machine that runs the stack.
- Mail to `void.test` still goes through `transport_maps` and the mailertable, because the sink listens on port 2525 and an MX record cannot name a port.
- RFC 6761 §6.2 item 1 warns that test names "are likely to yield different results on different networks". A network outside the stack may answer for `void.test`. Only the stack looks the test names up; the other tests pass them only as strings.

## References

- [test-environment.md, Interoperability environment](../../reference/test-environment.md#interoperability-environment)
- [Testing ADR 0001](0001-interop-tests-run-on-distro-mtas-in-compose-on-every-pr.md)
- RFC 2606 §2 and §3, RFC 6761 §6.2, §6.4, and §6.5
