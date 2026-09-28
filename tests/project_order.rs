use paper_codex::db::Database;

async fn siblings(db: &Database, parent: Option<&str>) -> Vec<String> {
    db.list_projects()
        .await
        .unwrap()
        .into_iter()
        .filter(|project| project.parent_id.as_deref() == parent)
        .map(|project| project.id)
        .collect()
}

#[tokio::test]
async fn project_order_persists_across_reopen_and_keeps_subtrees_and_papers() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite:{}?mode=rwc",
        directory.path().join("state.sqlite").display()
    );
    let db = Database::connect(&url).await.unwrap();
    let a = db.create_project("a", "A", "goal").await.unwrap();
    let b = db.create_project("b", "B", "").await.unwrap();
    let c = db
        .create_project_with_parent("c", "C", "", Some(&a))
        .await
        .unwrap();
    db.insert_paper("paper:one", "One").await.unwrap();
    db.add_paper_to_project("paper:one", &c).await.unwrap();
    db.move_project(&b, None, &[b.clone(), a.clone()])
        .await
        .unwrap();
    assert_eq!(siblings(&db, None).await, vec![b.clone(), a.clone()]);
    db.update_project(&b, "Z renamed", "", None).await.unwrap();
    assert_eq!(siblings(&db, None).await, vec![b.clone(), a.clone()]);
    db.move_project(&a, Some(&b), std::slice::from_ref(&a))
        .await
        .unwrap();
    assert_eq!(
        db.get_project(&c).await.unwrap().unwrap().parent_id,
        Some(a.clone())
    );
    assert_eq!(db.project_impact(&c).await.unwrap().direct_papers, 1);
    db.move_project(&a, None, &[b.clone(), a.clone()])
        .await
        .unwrap();
    drop(db);
    let db = Database::connect(&url).await.unwrap();
    assert_eq!(siblings(&db, None).await, vec![b, a]);
    let d = db.create_project("d", "A new project", "").await.unwrap();
    assert_eq!(siblings(&db, None).await.last(), Some(&d));
}

#[tokio::test]
async fn invalid_or_stale_moves_leave_the_tree_unchanged() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let a = db.create_project("a", "A", "").await.unwrap();
    let b = db.create_project("b", "B", "").await.unwrap();
    let c = db
        .create_project_with_parent("c", "C", "", Some(&a))
        .await
        .unwrap();
    let before = serde_json::to_value(db.list_projects().await.unwrap()).unwrap();
    for (source, parent, order) in [
        (&a, Some(a.as_str()), vec![a.clone(), c.clone()]),
        (&a, Some(c.as_str()), vec![a.clone()]),
        (&a, Some("missing"), vec![a.clone()]),
        (&a, None, vec![a.clone()]),
        (&a, None, vec![a.clone(), b.clone(), b.clone()]),
        (&a, None, vec![a.clone(), b.clone(), c.clone()]),
        (&c, Some(b.as_str()), vec![]),
    ] {
        assert!(db.move_project(source, parent, &order).await.is_err());
        assert_eq!(
            serde_json::to_value(db.list_projects().await.unwrap()).unwrap(),
            before
        );
    }
    assert!(db.move_project("missing", None, &[a, b]).await.is_err());
}

#[tokio::test]
async fn legacy_projects_get_neutral_order_without_losing_parent_or_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite:{}?mode=rwc",
        directory.path().join("legacy.sqlite").display()
    );
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    sqlx::raw_sql("CREATE TABLE projects (id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, name TEXT NOT NULL, purpose TEXT NOT NULL DEFAULT '', parent_id TEXT REFERENCES projects(id), created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP); INSERT INTO projects(id,slug,name,purpose) VALUES('a','a','A','goal'); INSERT INTO projects(id,slug,name,parent_id) VALUES('c','c','Child','a');")
        .execute(&pool).await.unwrap();
    pool.close().await;
    let db = Database::connect(&url).await.unwrap();
    assert_eq!(db.get_project("a").await.unwrap().unwrap().sort_order, 0);
    assert_eq!(db.get_project("a").await.unwrap().unwrap().purpose, "goal");
    assert_eq!(
        db.get_project("c")
            .await
            .unwrap()
            .unwrap()
            .parent_id
            .as_deref(),
        Some("a")
    );
    drop(db);
    let db = Database::connect(&url).await.unwrap();
    assert_eq!(db.list_projects().await.unwrap().len(), 2);
}
