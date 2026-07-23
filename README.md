# Paper Codex

[中文文档](README.zh-CN.md)

Paper Codex is a local-first workspace for reading papers, organizing research projects, and connecting evidence across a growing literature collection. It combines an integrated PDF reader, structured paper notes, a knowledge graph, and Codex-powered conversations in one local HTTP service.

> Paper Codex is designed for one researcher running one private workspace. It is not a hosted multi-user service.

## Highlights

- Add papers by title, DOI, arXiv identifier, URL, or local PDF.
- Keep related papers together in nested research projects; one paper may belong to multiple projects.
- Read PDFs inside the workspace with synchronized Codex explanations, citations, highlights, and overlays.
- Ask Codex questions scoped to the current paper, project, or entire library.
- Explore connections between papers, concepts, methods, datasets, and findings in a knowledge graph.
- Search extracted paper text with SQLite/FTS instead of maintaining a separate search service.
- Collapse and resize the file tree, reader, graph, and Codex panels to fit a laptop or desktop screen.

## How it works

Paper Codex is a single local process:

```text
Browser ──HTTP──> Rust/Axum service ──> SQLite/FTS + paper workspace
                         └────────────> Codex CLI (stdio)
```

- **Backend:** Rust, Axum, SQLite, PDF extraction, indexing, task processing, and API routes.
- **Frontend:** React, Vite, integrated PDF rendering, Codex conversation UI, and graph visualization.
- **Codex integration:** the service starts the Codex app server through the local Codex CLI and keeps conversation context scoped to the selected research material.
- **Default network boundary:** `127.0.0.1:3000`; the open-source service does not include a reverse proxy or public-domain configuration.

## Requirements

- Rust stable
- Node.js 20.19 or newer (Node.js 22 LTS recommended)
- SQLite 3
- A working Codex CLI installation, authenticated as the same operating-system user that runs Paper Codex
- `openssl` and `htpasswd` for generating local secrets

## Quick start from source

Clone the repository and install the frontend dependencies:

```bash
git clone https://github.com/wkj2333666/Paper-Codex.git
cd Paper-Codex

cd web
npm ci
npm run build
cd ..
```

Create a local configuration file and generate its two secrets:

```bash
cp paper-codex.env.example paper-codex.env

# bcrypt password hash
htpasswd -bnBC 12 "" 'replace-with-your-password' | tr -d ':\n'

# JWT signing secret
openssl rand -hex 32
```

Put the two command outputs into `PAPER_CODEX_PASSWORD_HASH` and `PAPER_CODEX_JWT_SECRET` in `paper-codex.env`. Then build and start the service from the repository root:

```bash
cargo build --release --locked

set -a
. ./paper-codex.env
set +a
./target/release/paper-codex
```

Open <http://127.0.0.1:3000>. The first run creates the Git-ignored `paper-workspace/` directory for PDFs, extracted text, indexes, notes, and the SQLite database.

## Codex setup

Make sure the Codex CLI is installed and authenticated for the same operating-system user that starts Paper Codex. If supported by your installation, the usual login command is:

```bash
codex login
```

The Codex panel exposes conversation scope, model, reasoning effort, and service speed when the connected Codex runtime supports them. Paper Codex does not store your Codex credentials in the repository.

## Data and privacy

- Runtime data lives in `paper-workspace/`, which is ignored by Git.
- Uploaded PDFs, extracted Markdown, SQLite indexes, notes, and conversation state may contain private research material.
- `paper-codex.env` contains secrets and is ignored; never commit it.
- The minimal service listens on localhost. If you expose it to another device, provide your own encrypted tunnel, VPN, or reverse proxy and authentication policy.

## Development

Run the focused checks while working on a change:

```bash
cd web
npm ci
npm test -- --run
npm run typecheck
npm run build
cd ..

cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

For the full local release check, use:

```bash
scripts/build-release.sh
```

The GitHub Actions workflow repeats these checks on pushes and pull requests. When a `v*` tag is pushed, it builds release archives for x86_64, aarch64, and armv7 Linux targets, so users do not need to compile the full Rust release locally.

## Releases

Release archives contain one target-specific backend binary, compiled web assets, and generic `systemd --user` templates. Each release provides x86_64, aarch64, and armv7 Linux archives. The archives are intentionally deployment-neutral: they do not contain a domain name, Caddy configuration, machine path, or private environment file.

For a local deployment, unpack a release into the project directory, point `PAPER_CODEX_STATIC_DIR` at its `web/` directory, and keep `paper-workspace/` and the environment file outside the release contents. The release workflow and templates in `deploy/` are the public reference; adapt them to your operating system and network boundary.

## Contributing

Issues, documentation improvements, bug fixes, and feature pull requests are welcome. Before opening a pull request:

1. Keep the change focused and explain the user impact.
2. Run the frontend and Rust checks above.
3. Do not include `paper-workspace/`, `.runtime/`, `node_modules/`, build output, secrets, or local deployment files.

## License

Paper Codex is released under the [MIT License](LICENSE).
