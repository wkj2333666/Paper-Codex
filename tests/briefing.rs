use chrono::{TimeZone, Utc};
use paper_codex::{
    briefing::{due, local_day, BriefingConfig, BriefingService, BriefingSettings},
    db::Database,
};

#[test]
fn briefing_yaml_defaults_and_validation() {
    let mut config: BriefingConfig =
        serde_yaml::from_str("project_id: project-a\nenabled: true\ntime: '09:30'\n").unwrap();
    assert!(config.validate().is_ok());
    assert!(!config.email_enabled);
    assert_eq!(config.max_papers, 12);
    config.email_enabled = true;
    config.recipient = "reader@example.test\r\nBcc: hidden@example.test".into();
    assert!(config.validate().is_err());
    config.recipient = "reader@example.test".into();
    assert!(config.validate().is_ok());
    config.categories = vec!["cs.RO OR all:everything".into()];
    assert!(config.validate().is_err());
    assert!(serde_yaml::from_str::<BriefingConfig>("enabeld: true").is_err());
}

#[test]
fn schedule_uses_shanghai_date_and_recovers_missed_time() {
    let config = BriefingConfig {
        enabled: true,
        ..Default::default()
    };
    assert!(!due(
        &config,
        Utc.with_ymd_and_hms(2026, 9, 27, 1, 29, 59).unwrap()
    ));
    assert!(due(
        &config,
        Utc.with_ymd_and_hms(2026, 9, 27, 1, 30, 0).unwrap()
    ));
    assert!(due(
        &config,
        Utc.with_ymd_and_hms(2026, 9, 27, 5, 0, 0).unwrap()
    ));
    assert_eq!(
        local_day(Utc.with_ymd_and_hms(2026, 9, 27, 16, 1, 0).unwrap()),
        "2026-09-28"
    );
    assert!(!due(&BriefingConfig::default(), Utc::now()));
}

#[test]
fn project_configs_require_an_owner_and_migrate_a_single_legacy_project() {
    let settings =
        BriefingSettings::parse(b"project_ids: [ei]\ntime: '08:00'\nenabled: true\n").unwrap();
    assert_eq!(settings.projects[0].project_id, "ei");
    assert_eq!(settings.projects[0].time, "08:00");
    assert!(settings.projects[0].enabled);
    let serialized = serde_yaml::to_string(&settings).unwrap();
    assert!(!serialized.contains("project_ids"));
    assert_eq!(
        BriefingSettings::parse(serialized.as_bytes())
            .unwrap()
            .projects
            .len(),
        1
    );
    assert!(BriefingSettings::parse(b"project_ids: []").is_err());
    assert!(BriefingSettings::parse(b"project_ids: [a,b]").is_err());
    assert!(BriefingSettings::parse(b"projects:\n - project_id: a\n - project_id: a\n").is_err());
    assert_eq!(
        BriefingSettings::parse(b"projects:\n - project_id: a\n - project_id: b\n")
            .unwrap()
            .projects
            .len(),
        2
    );
    assert!(BriefingSettings::parse(b"projects: []")
        .unwrap()
        .projects
        .is_empty());
}

#[tokio::test]
async fn same_day_and_seen_papers_are_independent_between_projects() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    BriefingService::recover_states(&db).await.unwrap();
    for project in ["ei", "vla"] {
        sqlx::query("INSERT INTO daily_briefings(id,project_id,day,status,started_at,settings_json) VALUES(?,?,'2026-09-27','completed','2026-09-27','{}')").bind(project).bind(project).execute(db.pool()).await.unwrap();
        sqlx::query("INSERT INTO briefing_seen VALUES(?,'arxiv:1234.56789','v1')")
            .bind(project)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM daily_briefings")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 2);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM briefing_seen")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 2);
    assert!(sqlx::query("INSERT INTO daily_briefings(id,project_id,day,status,started_at,settings_json) VALUES('duplicate','ei','2026-09-27','completed','2026-09-27','{}')").execute(db.pool()).await.is_err());
}

#[tokio::test]
async fn legacy_schema_migration_preserves_body_delivery_and_original_tables() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql("CREATE TABLE daily_briefings (id TEXT PRIMARY KEY, day TEXT UNIQUE NOT NULL,status TEXT NOT NULL,markdown TEXT NOT NULL DEFAULT '',error TEXT,conversation_id TEXT,mail_status TEXT NOT NULL DEFAULT 'pending',mail_attempts INTEGER NOT NULL DEFAULT 0,attempts INTEGER NOT NULL DEFAULT 0,started_at TEXT NOT NULL,completed_at TEXT,sources_json TEXT NOT NULL DEFAULT '[]',settings_json TEXT NOT NULL,next_attempt_at TEXT,mail_error TEXT); CREATE TABLE briefing_seen(paper_id TEXT,updated TEXT,PRIMARY KEY(paper_id,updated)); INSERT INTO daily_briefings(id,day,status,markdown,mail_status,mail_attempts,attempts,started_at,settings_json) VALUES('old','2026-09-27','completed','原文保持不变','sent',4,2,'2026-09-27','{}'); INSERT INTO briefing_seen VALUES('paper-a','v1');").execute(db.pool()).await.unwrap();
    BriefingService::recover_states(&db).await.unwrap();
    BriefingService::recover_states(&db).await.unwrap();
    let row: (String, String, i64, i64) = sqlx::query_as(
        "SELECT markdown,mail_status,mail_attempts,attempts FROM daily_briefings WHERE id='old'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row, ("原文保持不变".into(), "sent".into(), 4, 2));
    let legacy: i64 = sqlx::query_scalar("SELECT count(*) FROM briefing_seen_legacy_v1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(legacy, 1);
    let ei = db.create_project("ei", "EI", "").await.unwrap();
    let other = db.create_project("other", "Other", "").await.unwrap();
    BriefingService::adopt_legacy(&db, &ei).await.unwrap();
    BriefingService::adopt_legacy(&db, &other).await.unwrap();
    let owner: String = sqlx::query_scalar("SELECT project_id FROM daily_briefings WHERE id='old'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(owner, ei);
    let seen: (String, String) = sqlx::query_as("SELECT project_id,paper_id FROM briefing_seen")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(seen, (ei, "paper-a".into()));
}

#[tokio::test]
async fn restart_preserves_completed_text_and_does_not_blindly_resend() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    BriefingService::recover_states(&db).await.unwrap();
    for (id, status, mail) in [
        ("done", "completed", "sending"),
        ("interrupted", "running", "pending"),
        ("sent", "completed", "sent"),
    ] {
        sqlx::query("INSERT INTO daily_briefings(id,day,status,markdown,started_at,settings_json,mail_status) VALUES(?,?,?,'preserved','2026-09-27','{}',?)")
            .bind(id).bind(id).bind(status).bind(mail).execute(db.pool()).await.unwrap();
    }
    BriefingService::recover_states(&db).await.unwrap();
    let done: (String, String, String) =
        sqlx::query_as("SELECT status,mail_status,markdown FROM daily_briefings WHERE id='done'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        done,
        ("completed".into(), "uncertain".into(), "preserved".into())
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM daily_briefings WHERE id='interrupted'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(status, "failed");
    let sent: String =
        sqlx::query_scalar("SELECT mail_status FROM daily_briefings WHERE id='sent'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(sent, "sent");
    for updated in ["2026-09-26", "2026-09-26", "2026-09-27"] {
        sqlx::query("INSERT OR IGNORE INTO briefing_seen VALUES('project-a','arxiv:1234.56789',?)")
            .bind(updated)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM briefing_seen")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 2);
}
