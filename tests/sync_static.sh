#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
sync_script="${project_dir}/scripts/sync-static.sh"

if [[ ! -x "${sync_script}" ]]; then
  printf 'missing executable static deployment script: %s\n' "${sync_script}" >&2
  exit 1
fi

test_root="$(mktemp -d)"
trap 'rm -rf -- "${test_root}"' EXIT
source_dir="${test_root}/source"
target_dir="${test_root}/target"
mkdir -p "${source_dir}/assets" "${target_dir}/assets"
printf '<script src="/assets/new.js"></script>\n' >"${source_dir}/index.html"
printf 'new bundle\n' >"${source_dir}/assets/new.js"
printf 'old bundle\n' >"${target_dir}/assets/stale.js"

"${sync_script}" "${source_dir}" "${target_dir}"

cmp "${source_dir}/index.html" "${target_dir}/index.html"
cmp "${source_dir}/assets/new.js" "${target_dir}/assets/new.js"
test ! -e "${target_dir}/assets/stale.js"
printf 'static deployment replaces the release atomically and removes stale assets\n'
