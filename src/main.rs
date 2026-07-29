use anyhow::Result;
use paper_codex::{
    acquisition::Acquirer,
    api::{build_router, AppState},
    auth::Auth,
    codex::{CodexCommand, CodexRuntime},
    config::Config,
    conversation_engine::ConversationEngine,
    db::Database,
    research::ResearchProvider,
    research_providers::{research_http_client, ArxivProvider, CrossrefProvider, OpenAlexProvider},
    research_service::{ResearchService, ResearchServiceConfig},
    research_store::ResearchStore,
    tasks::TaskEngine,
    workspace::Workspace,
};
use std::{sync::Arc, time::Duration};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("paper_codex=info,tower_http=info")),
        )
        .init();
    let config = Config::from_env()?;
    let workspace = Workspace::initialize(&config.workspace).await?;
    let db = Database::connect(&config.database_url).await?;
    let auth = Auth::new(config.password_hash.clone(), config.jwt_secret.clone());
    let acquirer = Acquirer::new(config.max_upload_bytes)?;
    let research_store = ResearchStore::new(db.clone());
    let research_client = research_http_client()?;
    let providers: Vec<Arc<dyn ResearchProvider>> = vec![
        Arc::new(OpenAlexProvider::new(
            research_client.clone(),
            "https://api.openalex.org/".parse()?,
        )),
        Arc::new(CrossrefProvider::new(
            research_client.clone(),
            "https://api.crossref.org/".parse()?,
        )),
        Arc::new(ArxivProvider::new(
            research_client,
            "https://export.arxiv.org/".parse()?,
        )),
    ];
    let research = Arc::new(ResearchService::new(
        research_store.clone(),
        providers,
        Acquirer::new(config.research_pdf_max_bytes)?,
        ResearchServiceConfig {
            cache_dir: config.research_cache_dir.clone(),
            cache_max_bytes: config.research_cache_max_bytes,
            cache_ttl: Duration::from_secs(
                config.research_cache_ttl_days.saturating_mul(24 * 60 * 60),
            ),
            max_concurrency: config.research_max_concurrency,
        },
    )?);
    research.recover_interrupted_runs().await?;
    let pruned = research.prune_cache().await?;
    tracing::info!(
        removed_files = pruned.removed_files,
        removed_bytes = pruned.removed_bytes,
        remaining_bytes = pruned.remaining_bytes,
        "research cache pruned"
    );
    let codex = CodexRuntime::spawn(CodexCommand::app_server(
        config.codex_bin.clone(),
        config.codex_home.clone(),
        Some(config.runtime_tmp.clone()),
    ))
    .await?;
    let engine = TaskEngine::start_with_research(
        db.clone(),
        workspace.clone(),
        acquirer,
        codex.clone(),
        Some(research_store),
    )
    .await?;
    let conversation_engine = ConversationEngine::start_with_research(
        db.clone(),
        workspace.clone(),
        codex,
        Some(research.clone()),
    )
    .await?;
    let state = AppState::new(
        db,
        workspace,
        auth,
        engine,
        conversation_engine,
        config.static_dir.clone(),
        config.max_upload_bytes,
    )
    .with_research_service(research);
    let index = config.static_dir.join("index.html");
    let static_files = ServeDir::new(&config.static_dir).not_found_service(ServeFile::new(index));
    let app = build_router(state)
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(address=%config.bind, workspace=%config.workspace.display(), "Paper Codex listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
