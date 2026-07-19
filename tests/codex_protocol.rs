use paper_codex::codex::{CodexCommand, CodexRuntime, CodexTurn};
use std::path::PathBuf;
use tokio::sync::watch;

fn fake_command() -> CodexCommand {
    CodexCommand {
        program: PathBuf::from("python3"),
        args: vec![format!(
            "{}/fixtures/fake-app-server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        codex_home: None,
    }
}

#[tokio::test]
async fn initializes_starts_thread_and_streams_final_agent_text() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let mut events = runtime.subscribe();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let outcome = runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "summarize".into(),
                output_schema: None,
            },
            cancel_rx,
        )
        .await
        .unwrap();

    assert_eq!(outcome.thread_id, "thread-fake");
    assert_eq!(outcome.final_text, "structured answer");
    assert_eq!(events.recv().await.unwrap().kind, "agent-delta");
}

#[tokio::test]
async fn maps_cancellation_to_turn_interrupt() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    cancel_tx.send(true).unwrap();
    let outcome = runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "cancel-me".into(),
                output_schema: None,
            },
            cancel_rx,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "interrupted");
}

#[tokio::test]
async fn preserves_turn_failure_details() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let outcome = runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "fail-me".into(),
                output_schema: None,
            },
            cancel_rx,
        )
        .await
        .unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(
        outcome.error.as_deref(),
        Some("structured output rejected: schema mismatch")
    );
}
