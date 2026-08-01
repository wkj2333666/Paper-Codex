#!/usr/bin/env bash
set -euo pipefail

target="${PAPER_CODEX_CODEX_HOME:-$PWD/.runtime/codex-home}"
source_home=""
skill_sources=()

usage() {
  echo "usage: $0 [--target PATH] [--import-from CODEX_HOME] [--sync-skill-from SKILL_DIR]..."
}

config_uses_file_credentials() {
  awk '
    BEGIN { top_level = 1; isolated = 0 }
    /^[[:space:]]*\[/ { top_level = 0 }
    top_level && /^[[:space:]]*cli_auth_credentials_store[[:space:]]*=/ {
      isolated = ($0 ~ /^[[:space:]]*cli_auth_credentials_store[[:space:]]*=[[:space:]]*"file"[[:space:]]*(#.*)?$/)
    }
    END { exit(isolated ? 0 : 1) }
  ' "$1"
}

while (($#)); do
  case "$1" in
    --target)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      target="$2"
      shift 2
      ;;
    --import-from)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      source_home="$2"
      shift 2
      ;;
    --sync-skill-from)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      skill_sources+=("$2")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -n "$source_home" && "$source_home" == "$target" ]]; then
  echo "source and target Codex homes must differ" >&2
  exit 2
fi

install -d -m 0700 "$target"
install -d -m 0700 "$target/skills"

if [[ ! -e "$target/config.toml" ]]; then
  temporary="$(mktemp "$target/.config.toml.XXXXXX")"
  trap 'rm -f -- "$temporary"' EXIT
  printf '%s\n' 'cli_auth_credentials_store = "file"' > "$temporary"
  if [[ -n "$source_home" && -f "$source_home/config.toml" ]]; then
    awk '
      BEGIN { top_level = 1 }
      /^[[:space:]]*\[/ { top_level = 0 }
      top_level && /^[[:space:]]*(sqlite_home|cli_auth_credentials_store)[[:space:]]*=/ { next }
      { print }
    ' "$source_home/config.toml" >> "$temporary"
  fi
  chmod 0600 "$temporary"
  mv "$temporary" "$target/config.toml"
  trap - EXIT
fi

if ! config_uses_file_credentials "$target/config.toml"; then
  echo "existing config.toml must set cli_auth_credentials_store to file" >&2
  exit 1
fi

if [[ -n "$source_home" && -d "$source_home/skills" ]]; then
  while IFS= read -r -d '' skill; do
    name="${skill##*/}"
    if [[ ! -e "$target/skills/$name" && ! -L "$target/skills/$name" ]]; then
      cp -aL -- "$skill" "$target/skills/$name"
    fi
  done < <(find "$source_home/skills" -mindepth 1 -maxdepth 1 ! -name .system -print0)
fi

for skill_source in "${skill_sources[@]}"; do
  [[ -d "$skill_source" && -f "$skill_source/SKILL.md" ]] || {
    echo "skill source must contain SKILL.md: $skill_source" >&2
    exit 2
  }
  skill_source="$(cd "$skill_source" && pwd -P)"
  name="${skill_source##*/}"
  [[ -n "$name" && "$name" != "." && "$name" != ".." && "$name" != ".system" ]] || {
    echo "invalid skill directory name: $name" >&2
    exit 2
  }
  destination="$target/skills/$name"
  temporary="$(mktemp -d "$target/skills/.${name}.sync.XXXXXX")"
  cp -aL -- "$skill_source/." "$temporary/"
  backup=""
  if [[ -e "$destination" || -L "$destination" ]]; then
    backup="$(mktemp -d "$target/skills/.${name}.backup.XXXXXX")"
    rmdir "$backup"
    mv "$destination" "$backup"
  fi
  if mv "$temporary" "$destination"; then
    if [[ -n "$backup" ]]; then
      rm -rf -- "$backup"
    fi
  else
    if [[ -n "$backup" ]]; then
      mv "$backup" "$destination"
    fi
    rm -rf -- "$temporary"
    exit 1
  fi
done

echo "initialized isolated Codex home: $target"
