## Summary

<!-- What and why. Link the issue: Closes #N -->

## Checklist

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` clean
- [ ] `cargo nextest run --workspace --locked` green
- [ ] `cargo deny check` green — new deps reviewed for license + advisories
- [ ] Docs/ADRs updated if architecture or behavior changed
- [ ] No secrets, tokens or personal data committed

## Security notes

<!-- Required when touching authz, permissions, sandbox/egress, evidence or
     lifecycle paths: what changed and how it was verified. Write "N/A" if
     the PR does not touch a trust boundary. -->
