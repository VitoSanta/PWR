> **SUPERSEDED PROCESS GUIDANCE — 2026-09-12.** [CONTRIBUTING.md](../CONTRIBUTING.md) is the current contribution guidance. The body below is preserved as evidence of the earlier review rules; two of them no longer bind. The prohibition on external Python tools is superseded by principle 9 of [the project contract](../MASTER_SPEC.md), and durable decisions now amend the canonical documents instead of adding an ADR. The remaining requirements — narrow commits, a test per behaviour change, stated verification, no committed weights, secrets or credentials — still hold.

# Contribution Guidelines

Open an issue/design note before changing domain schemas, security policy, provider traits, or benchmark rules. Keep commits narrow; add tests for every behaviour change; update an ADR for durable architectural decisions. Do not add runtime Python, hidden network calls, model-name capability assumptions, or unmeasured performance claims.

PRs state requirement/heuristic/fact classification, affected data migrations, benchmark impact, security implications, and verification commands/results. Never commit model weights, repository secrets, raw private code traces, or credentials. Reviewers require deterministic tests and benchmark evidence for optimization claims.
