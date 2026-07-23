use paper_codex::codex::{CodexCommand, CodexRunSettings, CodexRuntime, CodexTurn};
use paper_codex::prompts::conversation_answer_schema;
use serde_json::Value;
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

fn standard_settings() -> CodexRunSettings {
    CodexRunSettings {
        model: "gpt-test".into(),
        reasoning_effort: "low".into(),
        service_tier: None,
    }
}

async fn next_test_turn_params(
    mut events: tokio::sync::broadcast::Receiver<paper_codex::codex::CodexEvent>,
) -> Value {
    loop {
        let event = events.recv().await.unwrap();
        if event.kind == "test/turn-params" {
            return event.payload["params"].clone();
        }
    }
}

#[tokio::test]
async fn advertises_model_effort_and_speed_capabilities() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let capabilities = runtime.capabilities();
    assert_eq!(capabilities.default.model, "gpt-test");
    assert_eq!(
        capabilities.models[0].supported_reasoning_efforts,
        vec!["low", "high"]
    );
    assert!(capabilities.models[0].supports_fast);
}

#[tokio::test]
async fn validates_model_effort_and_speed_combinations() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    assert!(runtime
        .validate_settings(&CodexRunSettings {
            model: "gpt-test".into(),
            reasoning_effort: "unsupported".into(),
            service_tier: None,
        })
        .is_err());
    assert!(runtime
        .validate_settings(&CodexRunSettings {
            model: "gpt-test".into(),
            reasoning_effort: "low".into(),
            service_tier: Some("unknown".into()),
        })
        .is_err());
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
                settings: standard_settings(),
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
                settings: standard_settings(),
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
                settings: standard_settings(),
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

#[tokio::test]
async fn resumes_thread_and_parses_two_structured_answers() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let first = runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "structured-turn-one".into(),
                output_schema: Some(conversation_answer_schema()),
                settings: standard_settings(),
            },
            cancel_rx.clone(),
        )
        .await
        .unwrap();
    let second = runtime
        .run_turn(
            CodexTurn {
                thread_id: Some(first.thread_id.clone()),
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "structured-turn-two".into(),
                output_schema: Some(conversation_answer_schema()),
                settings: standard_settings(),
            },
            cancel_rx,
        )
        .await
        .unwrap();

    assert_eq!(first.thread_id, "thread-fake");
    assert_ne!(first.turn_id, second.turn_id);
    assert_eq!(
        first.answer.as_ref().unwrap().answer_markdown,
        "结构化回答 [1]"
    );
    assert_eq!(second.answer.as_ref().unwrap().citations[0].page, 1);
}

#[tokio::test]
async fn rejects_invalid_structured_answer_json() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let result = runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "invalid-structured".into(),
                output_schema: Some(conversation_answer_schema()),
                settings: standard_settings(),
            },
            cancel_rx,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn sends_per_turn_model_effort_and_fast_service_tier() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let params = CodexRunSettings {
        model: "gpt-test".into(),
        reasoning_effort: "high".into(),
        service_tier: Some("priority".into()),
    };
    let events = next_test_turn_params(runtime.subscribe());
    runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "settings".into(),
                output_schema: None,
                settings: params,
            },
            cancel_rx,
        )
        .await
        .unwrap();
    let payload = tokio::time::timeout(std::time::Duration::from_secs(1), events)
        .await
        .unwrap();
    assert_eq!(payload["model"], "gpt-test");
    assert_eq!(payload["effort"], "high");
    assert_eq!(payload["serviceTier"], "priority");
}

#[tokio::test]
async fn omits_service_tier_for_standard_speed() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let events = next_test_turn_params(runtime.subscribe());
    runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: tempfile::tempdir().unwrap().path().to_path_buf(),
                prompt: "standard-settings".into(),
                output_schema: None,
                settings: standard_settings(),
            },
            cancel_rx,
        )
        .await
        .unwrap();
    let payload = tokio::time::timeout(std::time::Duration::from_secs(1), events)
        .await
        .unwrap();
    assert!(!payload.as_object().unwrap().contains_key("serviceTier"));
}
