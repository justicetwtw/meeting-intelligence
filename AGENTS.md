# Agent instructions

These instructions apply to the entire repository.

## Required reading

Before editing code, read:

1. `docs/product-spec.md`
2. `docs/architecture.md`
3. `docs/provider-contracts.md` when touching providers or canonical types
4. `docs/security.md` and `docs/privacy.md` for data or credential paths

## Engineering rules

1. Respect module boundaries. Do not reach across crates, services, or interface
   layers to save a small amount of code.
2. Never hard-code a provider implementation into React or another UI. UI code
   depends on application commands and canonical DTOs, not OpenAI, local-AI, or
   SQLite details.
3. Never hard-code or commit an API key, Slack token, signing key, credential,
   recording, transcript, voiceprint, or real meeting fixture.
4. Do not change the documented architecture silently. Update the relevant ADR
   or design document in the same pull request after a material decision has
   been agreed.
5. Prefer clear, proportional controls. Do not add substantial complexity for a
   remote theoretical risk without a realistic path and impact.
6. Escalate only product decisions with a material tradeoff: public contracts,
   privacy posture, trust boundaries, persistence format, deployment model, or
   irreversible migration behavior.
7. Every milestone and behavior change must include deterministic tests.
8. Do not use cloud agent tasks as the default development workflow. Work in the
   repository and use normal branches and pull requests unless explicitly asked.
9. Every change must keep mock mode operational without credentials, GPU,
   downloaded models, or paid/network AI services.
10. Never lower an assertion, delete coverage, or rewrite a fixture expectation
    merely to make CI green. Fix the contract or implementation.

## Scope of the bootstrap

The current repository is an engineering foundation. Do not implement full
transcription, speaker embeddings, a production Slack bot, accounts, billing,
telemetry, analytics, or a production backend as part of bootstrap maintenance.

## Validation

Run the checks listed in `docs/development.md` for every affected stack. Report
exact commands and results; do not imply a check or hosted workflow ran when it
did not.
