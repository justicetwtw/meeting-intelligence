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
