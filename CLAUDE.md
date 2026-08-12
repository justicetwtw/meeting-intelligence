# Claude repository guide

Follow [`AGENTS.md`](AGENTS.md) as the authoritative repository-wide agent
instructions.

Before implementation, read `docs/product-spec.md` and
`docs/architecture.md`. Preserve provider replaceability, canonical data types,
evidence references, privacy boundaries, and credential-free mock mode.

Do not begin a milestone until its contract and deterministic test strategy are
clear. Keep changes focused, test each milestone, and surface only material
product or architecture tradeoffs. Never call a paid AI API or connect to a live
Slack workspace during routine development or CI.

## Local-first validation

Local validation is currently the authoritative merge gate. After every
implementation change, run this command from the repository root:

```powershell
.\scripts\verify.ps1
```

Implementation validation is complete only when the command ends with
`LOCAL VALIDATION: PASS`.

Hosted GitHub Actions currently cannot start because of an account-level billing
lock. A job that reports `The job was not started because your account is locked
due to a billing issue.` and ran `0 steps` is an infrastructure/account-state
failure, not an application, test, or workflow-logic failure. Never respond by
changing application code, lowering assertions, deleting tests, or modifying the
workflow to bypass the lock.
