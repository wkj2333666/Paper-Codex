use anyhow::{bail, Context, Result};
use std::{env, net::SocketAddr, path::PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub workspace: PathBuf,
    pub static_dir: PathBuf,
    pub database_url: String,
    pub codex_bin: PathBuf,
    pub codex_home: Option<PathBuf>,
    pub password_hash: String,
    pub jwt_secret: String,
    pub max_upload_bytes: usize,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let root = env::current_dir().context("read current directory")?;
        let workspace = env::var_os("PAPER_CODEX_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("paper-workspace"));
        let bind: SocketAddr = env::var("PAPER_CODEX_BIND")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()
            .context("invalid PAPER_CODEX_BIND")?;
        if !bind.ip().is_loopback() {
            bail!("PAPER_CODEX_BIND must use a loopback address");
        }
        let database_url = env::var("PAPER_CODEX_DATABASE_URL").unwrap_or_else(|_| {
            format!(
                "sqlite://{}?mode=rwc",
                workspace.join(".paper-wiki/state.sqlite").display()
            )
        });
        Ok(Self {
            bind,
            workspace,
            static_dir: env::var_os("PAPER_CODEX_STATIC_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("web/dist")),
            database_url,
            codex_bin: env::var_os("PAPER_CODEX_CODEX_BIN")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("codex")),
            codex_home: env::var_os("PAPER_CODEX_CODEX_HOME").map(PathBuf::from),
            password_hash: env::var("PAPER_CODEX_PASSWORD_HASH")
                .context("PAPER_CODEX_PASSWORD_HASH is required")?,
            jwt_secret: env::var("PAPER_CODEX_JWT_SECRET")
                .context("PAPER_CODEX_JWT_SECRET is required")?,
            max_upload_bytes: env::var("PAPER_CODEX_MAX_UPLOAD_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100 * 1024 * 1024),
        })
    }
}
