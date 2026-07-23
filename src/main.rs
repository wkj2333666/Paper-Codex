use anyhow::Result;
use paper_codex::{
    acquisition::Acquirer,
    api::{build_router, AppState},
    auth::Auth,
    codex::{CodexCommand, CodexRuntime},
    config::Config,
    conversation_engine::ConversationEngine,
    db::Database,
    tasks::TaskEngine,
    workspace::Workspace,
};
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
    let codex = CodexRuntime::spawn(CodexCommand::app_server(
        config.codex_bin.clone(),
        config.codex_home.clone(),
    ))
    .await?;
    let engine = TaskEngine::start(db.clone(), workspace.clone(), acquirer, codex.clone()).await?;
    let conversation_engine =
        ConversationEngine::start(db.clone(), workspace.clone(), codex).await?;
    let state = AppState::new(
        db,
        workspace,
        auth,
        engine,
        conversation_engine,
        config.static_dir.clone(),
        config.max_upload_bytes,
    );
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
