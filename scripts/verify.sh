#!/usr/bin/env bash

set -Eeuo pipefail

current_stage="initialization"
repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"

fail() {
  local status=$?
  printf '\nLOCAL VALIDATION: FAIL (%s)\n' "$current_stage" >&2
  exit "$status"
}
trap fail ERR

stage() {
  current_stage="$1"
  shift
  printf '\n==> %s\n' "$current_stage"
  "$@"
}

for command_name in npm cargo rustc python3; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Required executable was not found: %s\n' "$command_name" >&2
    exit 1
  fi
done

stage "Toolchain versions" rustc --version --verbose
stage "TypeScript dependencies" npm ci
stage "TypeScript lint" npm run lint
stage "TypeScript format check" npm run format:check
stage "TypeScript typecheck" npm run typecheck
stage "TypeScript tests" npm test
stage "TypeScript production build" npm run build

stage "Rust format check" cargo fmt --all -- --check
stage "Rust clippy" cargo clippy --workspace --all-targets --all-features -- -D warnings
stage "Rust workspace build" cargo build --workspace --all-features
stage "Rust workspace tests" cargo test --workspace --all-features
stage "SQLite integration test" cargo test \
  --package meeting-intelligence-integration-tests \
  --test offline_pipeline \
  sqlite_storage_round_trips_canonical_meeting \
  -- --exact
stage "Offline mock pipeline test" cargo test \
  --package meeting-intelligence-integration-tests \
  --test offline_pipeline \
  fixture_pipeline_exports_markdown_and_json_without_guessing_speakers \
  -- --exact

current_stage="Offline mock pipeline executable"
printf '\n==> %s\n' "$current_stage"
output_directory="$repository_root/target/local-validation-output"
rm -rf -- "$output_directory"
cargo run --package meeting-intelligence-server -- fixtures "$output_directory"
test -s "$output_directory/meeting.md"
test -s "$output_directory/meeting.json"

current_stage="Python environment"
printf '\n==> %s\n' "$current_stage"
if [[ ! -x "$repository_root/.venv/bin/python" ]]; then
  python3 -m venv "$repository_root/.venv"
fi
venv_python="$repository_root/.venv/bin/python"
"$venv_python" -m pip install --disable-pip-version-check -e "services/local-ai[dev]"
stage "Python Ruff lint" "$venv_python" -m ruff check services/local-ai
stage "Python Ruff format check" "$venv_python" -m ruff format --check services/local-ai
stage "Python tests" "$venv_python" -m pytest services/local-ai

trap - ERR
printf '\nLOCAL VALIDATION: PASS\n'
