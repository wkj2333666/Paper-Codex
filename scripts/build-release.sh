#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$project_dir/web"
npm ci
npm test -- --run
npm run typecheck
npm run build
bash "$project_dir/tests/sync_static.sh"

cd "$project_dir"
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked

echo "Release ready: $project_dir/target/release/paper-codex"
