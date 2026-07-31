use anyhow::{bail, Context, Result};
use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

fn resolve_project_path(root: &Path, configured: Option<PathBuf>, default: &str) -> PathBuf {
    let path = configured.unwrap_or_else(|| PathBuf::from(default));
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub workspace: PathBuf,
    pub static_dir: PathBuf,
    pub database_url: String,
    pub codex_bin: PathBuf,
    pub codex_home: Option<PathBuf>,
    pub runtime_tmp: PathBuf,
    pub password_hash: String,
    pub jwt_secret: String,
    pub max_upload_bytes: usize,
    pub research_cache_dir: PathBuf,
    pub research_cache_max_bytes: u64,
    pub research_pdf_max_bytes: usize,
    pub research_max_concurrency: usize,
    pub research_cache_ttl_days: u64,
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
        let max_upload_bytes = env::var("PAPER_CODEX_MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100 * 1024 * 1024);
        let research_pdf_max_bytes = env::var("PAPER_CODEX_RESEARCH_PDF_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(100 * 1024 * 1024);
        if research_pdf_max_bytes > max_upload_bytes {
            bail!("PAPER_CODEX_RESEARCH_PDF_MAX_BYTES cannot exceed PAPER_CODEX_MAX_UPLOAD_BYTES");
        }
        let research_max_concurrency = env::var("PAPER_CODEX_RESEARCH_MAX_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3);
        if research_max_concurrency == 0 {
            bail!("PAPER_CODEX_RESEARCH_MAX_CONCURRENCY must be greater than zero");
        }
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
            codex_home: Some(resolve_project_path(
                &root,
                env::var_os("PAPER_CODEX_CODEX_HOME").map(PathBuf::from),
                ".runtime/codex-home",
            )),
            runtime_tmp: env::var_os("PAPER_CODEX_RUNTIME_TMP")
                .map(PathBuf::from)
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        root.join(path)
                    }
                })
                .unwrap_or_else(|| root.join(".runtime/tmp")),
            password_hash: env::var("PAPER_CODEX_PASSWORD_HASH")
                .context("PAPER_CODEX_PASSWORD_HASH is required")?,
            jwt_secret: env::var("PAPER_CODEX_JWT_SECRET")
                .context("PAPER_CODEX_JWT_SECRET is required")?,
            max_upload_bytes,
            research_cache_dir: env::var_os("PAPER_CODEX_RESEARCH_CACHE_DIR")
                .map(PathBuf::from)
                .map(|path| {
                    if path.is_absolute() {
                        path
                    } else {
                        root.join(path)
                    }
                })
                .unwrap_or_else(|| root.join(".runtime/research-cache")),
            research_cache_max_bytes: env::var("PAPER_CODEX_RESEARCH_CACHE_MAX_BYTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(1024 * 1024 * 1024),
            research_pdf_max_bytes,
            research_max_concurrency,
            research_cache_ttl_days: env::var("PAPER_CODEX_RESEARCH_CACHE_TTL_DAYS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn codex_home_defaults_inside_project_runtime() {
        let root = Path::new("/srv/paper-codex");
        assert_eq!(
            resolve_project_path(root, None, ".runtime/codex-home"),
            root.join(".runtime/codex-home")
        );
    }

    #[test]
    fn codex_home_resolves_relative_override_inside_project() {
        let root = Path::new("/srv/paper-codex");
        assert_eq!(
            resolve_project_path(
                root,
                Some(PathBuf::from("private/codex")),
                ".runtime/codex-home",
            ),
            root.join("private/codex")
        );
    }

    #[test]
    fn codex_home_preserves_absolute_override() {
        let root = Path::new("/srv/paper-codex");
        assert_eq!(
            resolve_project_path(
                root,
                Some(PathBuf::from("/var/lib/paper-codex/codex-home")),
                ".runtime/codex-home",
            ),
            PathBuf::from("/var/lib/paper-codex/codex-home")
        );
    }
}
