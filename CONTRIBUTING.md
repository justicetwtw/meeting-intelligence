# Contributing

Thank you for helping build Meeting Intelligence. The project is deliberately
mock-first while its contracts and privacy boundaries stabilize.

## Before making a change

1. Read [the product specification](docs/product-spec.md) and
   [architecture](docs/architecture.md).
2. Keep code within its documented module boundary.
3. Preserve offline mock mode. Tests and CI must not require credentials, a GPU,
   a paid service, or a network AI call.
4. Discuss a material product or architecture tradeoff in an issue before
   changing an established contract.

## Development workflow

Create a focused branch from `main`, add tests for each milestone, and open a
pull request. Use Conventional Commit-style prefixes when useful, but clear and
concise messages matter more than strict formatting.

Run the applicable checks before requesting review:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features

npm ci
npm run lint
npm run typecheck
npm test
npm run build

python -m ruff check services/local-ai
python -m ruff format --check services/local-ai
python -m pytest services/local-ai
```

Do not commit recordings, credentials, exported meetings, local databases, or
generated model files. Synthetic fixtures must contain no personal or sensitive
data.

## Pull requests

- Explain what changed and why.
- Identify affected module boundaries and contracts.
- List the exact checks you ran.
- Call out privacy, security, migration, or compatibility implications.
- Keep provider-specific implementation out of UI code.

See [SECURITY.md](SECURITY.md) for confidential vulnerability reporting. Do not
open a public issue for an undisclosed security vulnerability.
