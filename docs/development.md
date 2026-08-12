# Development

All routine development and CI use deterministic mocks. No API key, Slack
credential, GPU, Hugging Face token, paid service, or model download is needed.

## Prerequisites

- Git
- Node.js 22.12 or newer and npm
- Rust stable with `rustfmt` and `clippy`
- Python 3.12 or newer
- Docker Compose only when testing the optional self-host service foundation

## Frontend

From the repository root:

```bash
npm ci
npm run lint
npm run typecheck
npm test
npm run build
```

Use `npm run dev --workspace @meeting-intelligence/desktop` for the browser shell.
The desktop interface shows mock capability only; it does not accept or persist
provider keys.

## Rust workspace

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features
cargo test --workspace --all-features
```

Run the offline pipeline test and executable after the workspace crates are
installed:

```bash
cargo test --package meeting-intelligence-integration-tests --test offline_pipeline
cargo run --package meeting-intelligence-server -- fixtures target/mock-output
```

The command reads checked-in synthetic JSON, passes it through Meeting Core and
the mock intelligence provider, and writes deterministic Markdown and JSON
exports under `target/mock-output` without network access.

## Python local-AI service

```bash
python -m venv .venv
# Activate .venv using the command for your shell.
python -m pip install -e "services/local-ai[dev]"
python -m ruff check services/local-ai
python -m ruff format --check services/local-ai
python -m pytest services/local-ai
```

Run the mock service with the command documented in `services/local-ai/README.md`.
It should bind to loopback by default and expose only `/health` and
`/capabilities` during bootstrap.

## Optional self-host foundation

```bash
docker compose config
docker compose up --build local-ai
```

This starts only mock infrastructure. Do not add credentials to the Compose file
or commit a populated `.env` file.

## Clean-clone verification

Verification must use a fresh clone or detached export, install from the checked-
in locks/manifests, run every stack's checks, and execute the offline pipeline.
Report exact commands, pass counts, failures, and skipped checks. A local pass is
not a substitute for hosted GitHub Actions, and vice versa.
