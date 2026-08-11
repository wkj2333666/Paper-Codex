use paper_codex::{
    db::Database,
    project_readme::{ProjectReadmeError, ProjectReadmeStore},
    workspace::{atomic_write, Workspace},
};

#[tokio::test]
async fn migrates_project_markdown_and_rejects_stale_writes() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(root.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project_id = db
        .create_project("safe-project", "Project", "Purpose")
        .await
        .unwrap();
    atomic_write(
        &root.path().join("projects/safe-project/project.md"),
        b"# Legacy\n\nExisting notes.",
    )
    .await
    .unwrap();
    let store = ProjectReadmeStore::new(db, workspace);

    let first = store.read(&project_id).await.unwrap();
    assert_eq!(first.markdown, "# Legacy\n\nExisting notes.");
    assert!(root.path().join("projects/safe-project/README.md").is_file());

    let saved = store
        .write(&project_id, "# Updated", &first.revision)
        .await
        .unwrap();
    assert_eq!(saved.markdown, "# Updated");
    assert_ne!(saved.revision, first.revision);

    let stale = store
        .write(&project_id, "# Stale", &first.revision)
        .await
        .unwrap_err();
    assert!(matches!(stale, ProjectReadmeError::Conflict { .. }));
    assert_eq!(store.read(&project_id).await.unwrap().markdown, "# Updated");
}

#[tokio::test]
async fn refuses_a_project_slug_that_escapes_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::initialize(root.path()).await.unwrap();
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let project_id = db
        .create_project("../escape", "Unsafe", "")
        .await
        .unwrap();
    let store = ProjectReadmeStore::new(db, workspace);

    let error = store.read(&project_id).await.unwrap_err();

    assert!(matches!(error, ProjectReadmeError::InvalidProjectSlug));
    assert!(!root.path().join("escape/README.md").exists());
}
