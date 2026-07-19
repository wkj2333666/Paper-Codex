#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  printf 'usage: %s <built-static-directory> <production-static-directory>\n' "$0" >&2
  exit 2
fi

source_dir="${1%/}"
target_dir="${2%/}"
[[ "${source_dir}" == /* && "${target_dir}" == /* ]]
[[ -f "${source_dir}/index.html" && -d "${source_dir}/assets" ]]
[[ "${source_dir}" != "${target_dir}" ]]
case "${target_dir}" in
  /|/opt|/home|/usr|/var) printf 'refusing unsafe target: %s\n' "${target_dir}" >&2; exit 2 ;;
esac

target_parent="$(dirname -- "${target_dir}")"
target_name="$(basename -- "${target_dir}")"
install -d -m 0755 "${target_parent}"
stage_dir="$(mktemp -d "${target_parent}/.${target_name}.staging.XXXXXX")"
backup_dir="${target_parent}/.${target_name}.previous.$$"

cleanup() {
  status="$?"
  trap - EXIT
  [[ ! -e "${stage_dir}" ]] || rm -rf -- "${stage_dir}"
  if [[ "${status}" -ne 0 && ! -e "${target_dir}" && -e "${backup_dir}" ]]; then
    mv -- "${backup_dir}" "${target_dir}"
  fi
  exit "${status}"
}
trap cleanup EXIT

rsync -a --delete -- "${source_dir}/" "${stage_dir}/"
chmod -R a+rX,u+w "${stage_dir}"

if [[ -e "${target_dir}" ]]; then
  [[ ! -e "${backup_dir}" ]]
  mv -- "${target_dir}" "${backup_dir}"
fi
mv -- "${stage_dir}" "${target_dir}"
[[ ! -e "${backup_dir}" ]] || rm -rf -- "${backup_dir}"
trap - EXIT

printf 'installed static release: %s -> %s\n' "${source_dir}" "${target_dir}"
