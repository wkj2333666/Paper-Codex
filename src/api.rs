use crate::{
    auth::{require_auth, Auth},
    conversation_engine::ConversationEngine,
    conversations::{AnnotationAnchor, ConversationEvent, ConversationScopeInput},
    db::Database,
    domain::TaskEvent,
    login_limiter::LoginLimiter,
    search::SearchIndex,
    tasks::{IngestInput, QuestionInput, TaskEngine},
    workspace::{safe_key, Workspace},
};
use anyhow::Context;
use async_stream::stream;
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post, put},
    Json, Router,
};
use futures::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub workspace: Workspace,
    pub auth: Auth,
    pub login_limiter: LoginLimiter,
    pub engine: Option<Arc<TaskEngine>>,
    pub conversation_engine: Option<Arc<ConversationEngine>>,
    pub search: SearchIndex,
    pub static_dir: PathBuf,
    pub max_upload_bytes: usize,
}

impl AppState {
    pub fn new(
        db: Database,
        workspace: Workspace,
        auth: Auth,
        engine: Arc<TaskEngine>,
        conversation_engine: Arc<ConversationEngine>,
        static_dir: PathBuf,
        max_upload_bytes: usize,
    ) -> Self {
        Self {
            search: SearchIndex::new(db.clone()),
            db,
            workspace,
            auth,
            login_limiter: LoginLimiter::default(),
            engine: Some(engine),
            conversation_engine: Some(conversation_engine),
            static_dir,
            max_upload_bytes,
        }
    }
    pub fn for_test(db: Database, workspace: Workspace, auth: Auth) -> Self {
        Self {
            search: SearchIndex::new(db.clone()),
            db,
            workspace,
            auth,
            login_limiter: LoginLimiter::default(),
            engine: None,
            conversation_engine: None,
            static_dir: PathBuf::new(),
            max_upload_bytes: 10 * 1024 * 1024,
        }
    }

    pub fn with_conversation_engine(mut self, engine: Arc<ConversationEngine>) -> Self {
        self.conversation_engine = Some(engine);
        self
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error=%error, "API operation failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "operation failed".into(),
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        anyhow::Error::from(error).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"error":self.message}))).into_response()
    }
}

pub fn build_router(state: AppState) -> Router {
    let auth = state.auth.clone();
    let protected = Router::new()
        .route("/api/dashboard", get(dashboard))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route("/api/projects/{id}/impact", get(project_impact))
        .route(
            "/api/projects/{id}/papers/{*paper_id}",
            post(add_project_paper).delete(remove_project_paper),
        )
        .route("/api/papers", get(list_papers))
        .route("/api/trash", get(list_trash))
        .route("/api/paper", get(get_paper).delete(trash_paper))
        .route("/api/paper/impact", get(paper_impact))
        .route("/api/paper/restore", post(restore_paper))
        .route(
            "/api/paper/permanent",
            axum::routing::delete(permanently_delete_paper),
        )
        .route("/api/paper/pdf", get(get_pdf))
        .route("/api/paper/annotations", get(list_paper_annotations))
        .route("/api/relations", get(get_relations))
        .route("/api/graph", get(get_graph))
        .route("/api/intake", post(create_intake))
        .route("/api/intake/upload", post(upload_pdf))
        .route("/api/tasks", get(list_tasks))
        .route("/api/tasks/{id}", get(get_task).delete(dismiss_task))
        .route("/api/tasks/{id}/cancel", post(cancel_task))
        .route("/api/events", get(events))
        .route("/api/search", get(search))
        .route("/api/questions", post(question))
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/conversations/{id}",
            get(get_conversation).patch(update_conversation),
        )
        .route(
            "/api/conversations/{id}/scopes",
            put(replace_conversation_scopes),
        )
        .route(
            "/api/conversations/{id}/messages",
            post(create_conversation_message),
        )
        .route("/api/conversations/{id}/cancel", post(cancel_conversation))
        .route("/api/conversations/{id}/events", get(conversation_events))
        .route("/api/citations/{id}/pin", post(pin_citation))
        .route(
            "/api/annotations/{id}",
            axum::routing::patch(update_annotation),
        )
        .route(
            "/api/annotations/{id}/anchors",
            put(replace_annotation_anchors),
        )
        .route_layer(middleware::from_fn_with_state(auth, require_auth));
    Router::new()
        .route("/api/health", get(health))
        .route("/api/session", post(login))
        .merge(protected)
        .layer(DefaultBodyLimit::max(state.max_upload_bytes))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(
        json!({"status":"ok","codex":state.engine.is_some() || state.conversation_engine.is_some(),"version":env!("CARGO_PKG_VERSION")}),
    )
}

#[derive(Deserialize)]
struct LoginRequest {
    password: String,
}
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    let client_ip = login_client_ip(&headers);
    if let Some(retry_after) = state.login_limiter.check_at(client_ip, Instant::now()) {
        let retry_after_seconds = retry_after.as_secs().max(1);
        tracing::warn!(
            client_ip = %client_ip,
            retry_after_seconds,
            "login attempt throttled"
        );
        let mut response = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"登录尝试过于频繁，请稍后重试"})),
        )
            .into_response();
        response.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&retry_after_seconds.to_string())
                .expect("retry-after seconds must be a valid header value"),
        );
        return response;
    }

    match state.auth.login(request.password).await {
        Ok(token) => {
            state.login_limiter.clear(client_ip);
            Json(json!({"token":token})).into_response()
        }
        Err(_) => {
            state
                .login_limiter
                .record_failure_at(client_ip, Instant::now());
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"密码不正确"})),
            )
                .into_response()
        }
    }
}

fn login_client_ip(headers: &HeaderMap) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

async fn dashboard(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let papers = state.db.list_papers().await?;
    let projects = state.db.list_projects().await?;
    let tasks = state.db.list_tasks(30).await?;
    let mut project_memberships = std::collections::BTreeMap::new();
    for project in &projects {
        project_memberships.insert(
            project.id.clone(),
            state.db.project_paper_ids(&project.id).await?,
        );
    }
    let mut inbox = Vec::new();
    for paper in &papers {
        if paper.note_path.is_some() && state.db.paper_project_ids(&paper.id).await?.is_empty() {
            inbox.push(paper.clone());
        }
    }
    let trash_count = state.db.list_trashed_papers().await?.len();
    Ok(Json(
        json!({"papers":papers,"projects":projects,"tasks":tasks,"inbox":inbox,"trash_count":trash_count,"project_memberships":project_memberships}),
    ))
}

async fn list_papers(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.list_papers().await?)))
}
async fn list_trash(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.list_trashed_papers().await?)))
}
async fn list_projects(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.list_projects().await?)))
}

#[derive(Deserialize)]
struct ProjectRequest {
    name: String,
    purpose: Option<String>,
    parent_id: Option<String>,
}
async fn create_project(
    State(state): State<AppState>,
    Json(request): Json<ProjectRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if request.name.trim().is_empty() {
        return Err(ApiError::bad_request("项目名称不能为空"));
    }
    let base_slug = slugify(&request.name);
    let base_slug = if base_slug.is_empty() {
        "project".to_owned()
    } else {
        base_slug
    };
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("{}-{}", base_slug, &suffix[..8]);
    let id = state
        .db
        .create_project_with_parent(
            &slug,
            request.name.trim(),
            request.purpose.as_deref().unwrap_or(""),
            request.parent_id.as_deref(),
        )
        .await?;
    let project = state
        .db
        .get_project(&id)
        .await?
        .context("created project missing")?;
    let project_dir = state.workspace.root().join("projects").join(&slug);
    tokio::fs::create_dir_all(project_dir.join("synthesis")).await?;
    crate::workspace::atomic_write(
        &project_dir.join("project.md"),
        format!("# {}\n\n{}\n", project.name, project.purpose).as_bytes(),
    )
    .await?;
    crate::workspace::atomic_write(&project_dir.join("papers.md"), b"# Papers\n").await?;
    state
        .search
        .upsert("project", &id, &project.name, &project.purpose)
        .await?;
    Ok((StatusCode::CREATED, Json(json!(project))))
}

async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .db
        .get_project(&id)
        .await?
        .map(|project| Json(json!(project)))
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "项目不存在".into(),
        })
}

#[derive(Deserialize)]
struct UpdateProjectRequest {
    name: String,
    purpose: String,
    parent_id: Option<String>,
}

async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<Value>, ApiError> {
    state
        .db
        .update_project(
            &id,
            &request.name,
            &request.purpose,
            request.parent_id.as_deref(),
        )
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    get_project(State(state), Path(id)).await
}

#[derive(Deserialize)]
struct DeleteProjectQuery {
    mode: Option<String>,
}

async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteProjectQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .delete_project(&id, query.mode.as_deref() == Some("subtree"))
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn project_impact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.project_impact(&id).await?)))
}

async fn add_project_paper(
    State(state): State<AppState>,
    Path((id, paper_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .add_paper_to_project(paper_id.trim_start_matches('/'), &id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_project_paper(
    State(state): State<AppState>,
    Path((id, paper_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .remove_paper_from_project(paper_id.trim_start_matches('/'), &id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PaperQuery {
    id: String,
}
async fn get_paper(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<Json<Value>, ApiError> {
    let paper = state
        .db
        .get_paper(&query.id)
        .await?
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "paper not found".into(),
        })?;
    let mut analysis = state.db.paper_analysis(&query.id).await?;
    if analysis.is_none() {
        if let Some(path) = &paper.note_path {
            if let Ok(note) = tokio::fs::read_to_string(path).await {
                analysis = crate::knowledge::analysis_from_markdown(&note);
                if let Some(value) = &analysis {
                    state
                        .db
                        .upsert_paper_analysis(
                            &query.id,
                            paper.canonical_sha256.as_deref().unwrap_or("legacy"),
                            value,
                        )
                        .await?;
                }
            }
        }
    }
    let projects = state.db.paper_project_ids(&query.id).await?;
    let relations = state.db.relations_for(&query.id).await?;
    Ok(Json(
        json!({"paper":paper,"analysis":analysis,"projects":projects,"relations":relations}),
    ))
}

async fn paper_impact(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.paper_impact(&query.id).await?)))
}

async fn trash_paper(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<StatusCode, ApiError> {
    state.db.trash_paper(&query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore_paper(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<StatusCode, ApiError> {
    state.db.restore_paper(&query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn permanently_delete_paper(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<StatusCode, ApiError> {
    let paper = state
        .db
        .get_paper(&query.id)
        .await?
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "论文不存在".into(),
        })?;
    state
        .db
        .permanently_delete_paper(&query.id)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let key = safe_key(&paper.id);
    remove_if_present(
        &state
            .workspace
            .root()
            .join("library/generated/papers")
            .join(format!("{key}.md")),
        false,
    )
    .await?;
    remove_if_present(
        &state
            .workspace
            .root()
            .join("library/catalog/papers")
            .join(format!("{key}.json")),
        false,
    )
    .await?;
    remove_if_present(
        &state.workspace.root().join("library/raw/papers").join(key),
        true,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_if_present(path: &std::path::Path, directory: bool) -> Result<(), ApiError> {
    let result = if directory {
        tokio::fs::remove_dir_all(path).await
    } else {
        tokio::fs::remove_file(path).await
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn get_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaperQuery>,
) -> Result<Response, ApiError> {
    let paper = state
        .db
        .get_paper(&query.id)
        .await?
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "paper not found".into(),
        })?;
    let sha = paper.canonical_sha256.context("paper has no revision")?;
    let path = state
        .workspace
        .root()
        .join("library/raw/papers")
        .join(safe_key(&paper.id))
        .join("revisions")
        .join(&sha)
        .join("paper.pdf");
    Ok(crate::pdf_range::pdf_response(&path, &sha, &headers).await?)
}

async fn list_paper_annotations(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.paper_annotations(&query.id).await?)))
}

async fn pin_citation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let annotation = state
        .db
        .pin_citation(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("引用不存在"))?;
    Ok(Json(json!(annotation)))
}

#[derive(Deserialize)]
struct UpdateAnnotationRequest {
    state: String,
}

async fn update_annotation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAnnotationRequest>,
) -> Result<Json<Value>, ApiError> {
    let annotation = state
        .db
        .set_annotation_state(&id, &request.state)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?
        .ok_or_else(|| ApiError::not_found("批注不存在"))?;
    Ok(Json(json!(annotation)))
}

#[derive(Deserialize)]
struct AnnotationAnchorInput {
    page: i64,
    rect_index: i64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct ReplaceAnnotationAnchorsRequest {
    anchors: Vec<AnnotationAnchorInput>,
}

async fn replace_annotation_anchors(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReplaceAnnotationAnchorsRequest>,
) -> Result<StatusCode, ApiError> {
    let anchors = request
        .anchors
        .into_iter()
        .map(|anchor| AnnotationAnchor {
            annotation_id: id.clone(),
            page: anchor.page,
            rect_index: anchor.rect_index,
            x: anchor.x,
            y: anchor.y,
            width: anchor.width,
            height: anchor.height,
        })
        .collect::<Vec<_>>();
    state
        .db
        .replace_annotation_anchors(&id, &anchors)
        .await
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_relations(
    State(state): State<AppState>,
    Query(query): Query<PaperQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.relations_for(&query.id).await?)))
}

#[derive(Deserialize)]
struct GraphQuery {
    project_id: Option<String>,
    paper_id: Option<String>,
    kinds: Option<String>,
    include_hypotheses: Option<bool>,
}

async fn get_graph(
    State(state): State<AppState>,
    Query(query): Query<GraphQuery>,
) -> Result<Json<Value>, ApiError> {
    for paper in state.db.papers_without_graph().await? {
        let mut analysis = state.db.paper_analysis(&paper.id).await?;
        if analysis.is_none() {
            if let Some(path) = &paper.note_path {
                if let Ok(note) = tokio::fs::read_to_string(path).await {
                    analysis = crate::knowledge::analysis_from_markdown(&note);
                    if let Some(value) = &analysis {
                        state
                            .db
                            .upsert_paper_analysis(
                                &paper.id,
                                paper.canonical_sha256.as_deref().unwrap_or("legacy"),
                                value,
                            )
                            .await?;
                    }
                }
            }
        }
        if let Some(analysis) = analysis {
            let graph =
                crate::graph::materialize_legacy_analysis(&paper.id, &paper.title, &analysis);
            state
                .db
                .replace_paper_graph(
                    &paper.id,
                    paper.canonical_sha256.as_deref().unwrap_or("legacy"),
                    &graph.nodes,
                    &graph.edges,
                )
                .await?;
        }
    }
    let kinds = query
        .kinds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    Ok(Json(json!(
        state
            .db
            .query_graph(
                query.project_id.as_deref(),
                query.paper_id.as_deref(),
                &kinds,
                query.include_hypotheses.unwrap_or(true),
            )
            .await?
    )))
}

#[derive(Deserialize)]
struct IntakeRequest {
    source: String,
    project_id: Option<String>,
}
async fn create_intake(
    State(state): State<AppState>,
    Json(request): Json<IntakeRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if request.source.trim().is_empty() {
        return Err(ApiError::bad_request("paper name or link is required"));
    }
    let engine = state
        .engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("task engine unavailable"))?;
    let id = engine
        .create_ingest(IngestInput {
            source: request.source,
            project_id: request.project_id,
            upload_path: None,
        })
        .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"task_id":id}))))
}

async fn upload_pdf(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let mut bytes = None;
    let mut filename = "uploaded.pdf".to_owned();
    let mut project_id = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        match field.name() {
            Some("file") => {
                filename = field.file_name().unwrap_or("uploaded.pdf").to_owned();
                bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            Some("project_id") => {
                project_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::bad_request(e.to_string()))?,
                );
            }
            _ => {}
        }
    }
    let bytes = bytes.ok_or_else(|| ApiError::bad_request("PDF file is required"))?;
    crate::acquisition::validate_pdf_bytes(&bytes, state.max_upload_bytes)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let upload_dir = state.workspace.state_dir().join("uploads");
    tokio::fs::create_dir_all(&upload_dir).await?;
    let path = upload_dir.join(format!("{}-{}", uuid::Uuid::new_v4(), safe_key(&filename)));
    crate::workspace::atomic_write(&path, &bytes).await?;
    let engine = state
        .engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("task engine unavailable"))?;
    let id = engine
        .create_ingest(IngestInput {
            source: filename,
            project_id: project_id.filter(|v| !v.is_empty()),
            upload_path: Some(path),
        })
        .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"task_id":id}))))
}

async fn list_tasks(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.list_tasks(100).await?)))
}
async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .db
        .get_task(&id)
        .await?
        .map(|task| Json(json!(task)))
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: "task not found".into(),
        })
}
async fn dismiss_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let task = state
        .db
        .get_task(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    if !matches!(task.state.as_str(), "done" | "failed" | "cancelled") {
        return Err(ApiError::conflict(
            "running tasks must be cancelled before dismissal",
        ));
    }
    if !state.db.dismiss_task(&id).await? {
        return Err(ApiError::not_found("task not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn cancel_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("task engine unavailable"))?
        .cancel(&id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ConversationListQuery {
    archived: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_conversations(
    State(state): State<AppState>,
    Query(query): Query<ConversationListQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        state
            .db
            .list_conversations(
                query.archived.unwrap_or(false),
                query.limit.unwrap_or(50),
                query.offset.unwrap_or(0),
            )
            .await?
    )))
}

#[derive(Deserialize)]
struct CreateConversationRequest {
    title: String,
    scopes: Vec<ConversationScopeInput>,
}

async fn create_conversation(
    State(state): State<AppState>,
    Json(request): Json<CreateConversationRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if request.title.trim().is_empty() || request.scopes.is_empty() {
        return Err(ApiError::bad_request("标题和上下文范围不能为空"));
    }
    let conversation = state
        .conversation_engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("conversation engine unavailable"))?
        .create_conversation(&request.title, request.scopes)
        .await
        .map_err(conversation_api_error)?;
    Ok((StatusCode::CREATED, Json(json!(conversation))))
}

#[derive(Deserialize)]
struct MessagePageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<MessagePageQuery>,
) -> Result<Json<Value>, ApiError> {
    let conversation = state
        .db
        .get_conversation(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("对话不存在"))?;
    let scopes = state.db.conversation_scopes(&id).await?;
    let messages = state
        .db
        .conversation_messages(&id, query.limit.unwrap_or(100), query.offset.unwrap_or(0))
        .await?;
    let mut messages_with_citations = Vec::with_capacity(messages.len());
    for message in messages {
        let citations = state.db.message_citations(&message.id).await?;
        messages_with_citations.push(json!({"id":message.id,"conversation_id":message.conversation_id,"role":message.role,"content":message.content,"turn_id":message.turn_id,"status":message.status,"error":message.error,"created_at":message.created_at,"updated_at":message.updated_at,"citations":citations}));
    }
    Ok(Json(json!({
        "conversation": conversation,
        "scopes": scopes,
        "messages": messages_with_citations,
    })))
}

#[derive(Deserialize)]
struct UpdateConversationRequest {
    title: Option<String>,
    archived: Option<bool>,
}

async fn update_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateConversationRequest>,
) -> Result<Json<Value>, ApiError> {
    let conversation = state
        .db
        .update_conversation(&id, request.title.as_deref(), request.archived)
        .await
        .map_err(conversation_api_error)?
        .ok_or_else(|| ApiError::not_found("对话不存在"))?;
    Ok(Json(json!(conversation)))
}

#[derive(Deserialize)]
struct ReplaceScopesRequest {
    scopes: Vec<ConversationScopeInput>,
}

async fn replace_conversation_scopes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReplaceScopesRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.scopes.is_empty() {
        return Err(ApiError::bad_request("上下文范围不能为空"));
    }
    if state.db.conversation_has_pending_turn(&id).await? {
        return Err(ApiError::conflict("回答生成期间不能修改上下文范围"));
    }
    state
        .db
        .replace_conversation_scopes(&id, &request.scopes)
        .await
        .map_err(conversation_api_error)?;
    Ok(Json(json!(state.db.conversation_scopes(&id).await?)))
}

#[derive(Deserialize)]
struct CreateMessageRequest {
    content: String,
}

async fn create_conversation_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateMessageRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let message = state
        .conversation_engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("conversation engine unavailable"))?
        .enqueue_message(&id, &request.content)
        .await
        .map_err(conversation_api_error)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"message_id":message.id,"status":message.status})),
    ))
}

async fn cancel_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if state.db.get_conversation(&id).await?.is_none() {
        return Err(ApiError::not_found("对话不存在"));
    }
    state
        .conversation_engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("conversation engine unavailable"))?
        .cancel(&id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn conversation_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if state.db.get_conversation(&id).await?.is_none() {
        return Err(ApiError::not_found("对话不存在"));
    }
    let replay = state
        .db
        .conversation_events_after(&id, query.after.unwrap_or(0))
        .await?;
    let mut live = state
        .conversation_engine
        .as_ref()
        .map(|engine| engine.subscribe());
    let stream = stream! {
        for item in replay { yield Ok(to_conversation_sse(item)); }
        if let Some(receiver) = &mut live {
            loop {
                match receiver.recv().await {
                    Ok(item) if item.conversation_id == id => yield Ok(to_conversation_sse(item)),
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn to_conversation_sse(event: ConversationEvent) -> Event {
    Event::default()
        .id(event.id.to_string())
        .event(event.event_type)
        .data(
            json!({
                "conversation_id":event.conversation_id,
                "message_id":event.message_id,
                "payload":event.payload,
                "created_at":event.created_at,
            })
            .to_string(),
        )
}

fn conversation_api_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    if message.contains("does not exist") || message.contains("不存在") {
        ApiError::not_found("对话或上下文不存在")
    } else if message.contains("busy") {
        ApiError::conflict("当前对话正在生成回答")
    } else {
        ApiError::bad_request(message)
    }
}

#[derive(Deserialize)]
struct EventsQuery {
    after: Option<i64>,
}
async fn events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let replay = state.db.events_after(query.after.unwrap_or(0)).await?;
    let mut live = state.engine.as_ref().map(|engine| engine.subscribe());
    let stream = stream! {
        for item in replay { yield Ok(to_sse(item)); }
        if let Some(receiver) = &mut live {
            loop { match receiver.recv().await { Ok(item) => yield Ok(to_sse(item)), Err(broadcast::error::RecvError::Lagged(_)) => continue, Err(_) => break } }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn to_sse(event: TaskEvent) -> Event {
    Event::default().id(event.id.to_string()).event(event.event_type).data(json!({"task_id":event.task_id,"payload":serde_json::from_str::<Value>(&event.payload_json).unwrap_or(Value::Null),"created_at":event.created_at}).to_string())
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    entity_type: Option<String>,
}
async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        state
            .search
            .query(&query.q, query.entity_type.as_deref())
            .await?
    )))
}

async fn question(
    State(state): State<AppState>,
    Json(request): Json<QuestionInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if request.question.trim().is_empty() {
        return Err(ApiError::bad_request("question is required"));
    }
    if let Some(engine) = &state.conversation_engine {
        let scope = ConversationScopeInput {
            scope_type: request.scope_type.clone(),
            scope_id: request.scope_id.clone(),
        };
        let title = request.question.chars().take(32).collect::<String>();
        let conversation = engine
            .create_conversation(&title, vec![scope])
            .await
            .map_err(conversation_api_error)?;
        let message = engine
            .enqueue_message(&conversation.id, &request.question)
            .await
            .map_err(conversation_api_error)?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(
                json!({"task_id":message.id,"conversation_id":conversation.id,"message_id":message.id}),
            ),
        ));
    }
    let id = state
        .engine
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("task engine unavailable"))?
        .create_question(request)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"task_id":id}))))
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut dash = false;
    for c in value.trim().to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
            dash = false;
        } else if !dash && !result.is_empty() {
            result.push('-');
            dash = true;
        }
    }
    result.trim_end_matches('-').to_owned()
}
