use chrono::{TimeZone, Utc};
use paper_codex::{
    briefing::{due, local_day, BriefingConfig, BriefingService},
    db::Database,
};

#[test]
fn briefing_yaml_defaults_and_validation() {
    let mut config: BriefingConfig =
        serde_yaml::from_str("enabled: true\ntime: '09:30'\n").unwrap();
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
        sqlx::query("INSERT OR IGNORE INTO briefing_seen VALUES('arxiv:1234.56789',?)")
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
