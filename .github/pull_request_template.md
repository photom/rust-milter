## Summary

<!-- What this pull request changes, and why. -->

## Related issues

<!-- "Closes #123", or "None". -->

## Testing

<!-- How you checked the change. Mark what you ran. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo deny --all-features check`
- [ ] Interoperability tests (`docker compose -f tests/interop/compose.yaml run --build --rm runner`)

## Checklist

- [ ] Documentation in `docs/` matches the change.
- [ ] A design decision has an ADR in `docs/adr/` or `docs/testing/adr/`.
- [ ] Email addresses in tests and examples use `void.invalid`.
- [ ] No secrets, keys, or real email addresses are committed.
