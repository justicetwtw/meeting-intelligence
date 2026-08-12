# Development

All routine development and CI use deterministic mocks. No API key, Slack
credential, GPU, Hugging Face token, paid service, or model download is needed.

## Prerequisites

- Git
- Node.js 22.12 or newer and npm
- Rust stable with `rustfmt` and `clippy`
- On Windows: Visual Studio Build Tools with the MSVC C++ x64/x86 tools and a
  Windows SDK
- Python 3.12 or newer
- Docker Compose only when testing the optional self-host service foundation

## Local-first validation

Local validation is currently the authoritative merge gate. On Windows, run the
complete repository verification from the repository root:

```powershell
.\scripts\verify.ps1
```

The script installs locked frontend dependencies, loads the Visual Studio build
environment, checks every TypeScript, Rust, SQLite, offline-pipeline, and Python
stage documented below, and exits nonzero at the first failed stage. A successful
run ends with:

```text
LOCAL VALIDATION: PASS
```

Unix-like environments may run `./scripts/verify.sh`; the PowerShell command is
the authoritative local verifier for the current Windows-first workflow.

Hosted GitHub Actions remain checked in, but currently cannot start because of
an account-level billing lock. If GitHub reports:

```text
The job was not started because your account is locked due to a billing issue.
```

and the job ran `0 steps`, this is infrastructure/account state. It is not an
application failure, test failure, or workflow-logic failure. Do not change
application code, lower assertions, delete tests, or modify workflows to work
around the billing lock. Required hosted checks can be restored as merge gates
after the account lock is resolved.

## Frontend

From the repository root:

```bash
npm ci
npm run lint
npm run format:check
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
cargo test --package meeting-intelligence-integration-tests --test offline_pipeline sqlite_storage_round_trips_canonical_meeting -- --exact
cargo test --package meeting-intelligence-integration-tests --test offline_pipeline fixture_pipeline_exports_markdown_and_json_without_guessing_speakers -- --exact
cargo test --package meeting-intelligence-integration-tests --test offline_pipeline
cargo run --package meeting-intelligence-server -- fixtures target/local-validation-output
```

The command reads checked-in synthetic JSON, passes it through Meeting Core and
the mock intelligence provider, and writes deterministic Markdown and JSON
exports under `target/local-validation-output` without network access.

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

Verification must use the checked-in locks/manifests, run every stack's checks,
and execute the offline pipeline. Use a fresh clone or detached export for
release verification. Report exact commands, pass counts, failures, and skipped
checks. During the documented account-level billing lock, the complete local
verification is the authoritative merge gate; do not describe unavailable
hosted checks as passing.
