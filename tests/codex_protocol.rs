use async_trait::async_trait;
use paper_codex::codex::{CodexCommand, CodexRunSettings, CodexRuntime, CodexTurn};
use paper_codex::codex_tools::{
    DynamicToolCall, DynamicToolDefinition, DynamicToolHandler, DynamicToolSession,
};
use paper_codex::prompts::conversation_answer_schema;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{mpsc, watch, Mutex};

fn fake_command() -> CodexCommand {
    CodexCommand {
        program: PathBuf::from("python3"),
        args: vec![format!(
            "{}/fixtures/fake-app-server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        codex_home: None,
        runtime_tmp: None,
    }
}

fn fake_command_without_dynamic_tools() -> CodexCommand {
    let mut command = fake_command();
    command.args.push("--reject-dynamic-tools".into());
    command
}

fn standard_settings() -> CodexRunSettings {
    CodexRunSettings {
        model: "gpt-test".into(),
        reasoning_effort: "low".into(),
        service_tier: None,
    }
}

fn research_turn(prompt: &str) -> CodexTurn {
    CodexTurn {
        thread_id: None,
        cwd: tempfile::tempdir().unwrap().keep(),
        prompt: prompt.to_owned(),
        output_schema: None,
        settings: standard_settings(),
    }
}

#[derive(Default)]
struct RecordingHandler {
    calls: Mutex<Vec<DynamicToolCall>>,
}

impl RecordingHandler {
    async fn calls(&self) -> Vec<DynamicToolCall> {
        self.calls.lock().await.clone()
    }
}

#[async_trait]
impl DynamicToolHandler for RecordingHandler {
    async fn call(&self, call: DynamicToolCall) -> anyhow::Result<Vec<Value>> {
        let query = call.arguments["query"].as_str().unwrap_or_default();
        self.calls.lock().await.push(call);
        Ok(vec![serde_json::json!({
            "query": query,
            "works": [{"title": "Rule Complexity", "evidence_level": "abstract"}]
        })])
    }
}

fn test_tool_session(handler: Arc<RecordingHandler>) -> DynamicToolSession {
    DynamicToolSession {
        definitions: vec![DynamicToolDefinition {
            name: "research_search".into(),
            description: "检索当前项目的相关论文".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["query", "reason"],
                "properties": {
                    "query": {"type": "string"},
                    "reason": {"type": "string"}
                }
            }),
        }],
        handler,
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

async fn next_test_runtime_tmp(
    mut events: tokio::sync::broadcast::Receiver<paper_codex::codex::CodexEvent>,
) -> PathBuf {
    loop {
        let event = events.recv().await.unwrap();
        if event.kind == "test/runtime-tmp" {
            return PathBuf::from(event.payload["params"]["path"].as_str().unwrap());
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
    assert!(capabilities.supports_dynamic_tools);
}

#[tokio::test]
async fn executes_dynamic_tool_requests_through_the_bound_handler() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let handler = Arc::new(RecordingHandler::default());
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let outcome = runtime
        .run_turn_with_events_and_tools(
            research_turn("call-research-search"),
            cancel_rx,
            event_tx,
            Some(test_tool_session(handler.clone())),
        )
        .await
        .unwrap();

    let calls = handler.calls().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].tool, "research_search");
    assert_eq!(calls[0].thread_id, "thread-fake");
    assert_eq!(outcome.status, "completed");
    assert_eq!(outcome.final_text, "tool-backed answer");
}

#[tokio::test]
async fn denies_non_tool_server_requests_without_invoking_the_handler() {
    let runtime = CodexRuntime::spawn(fake_command()).await.unwrap();
    let handler = Arc::new(RecordingHandler::default());
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let outcome = runtime
        .run_turn_with_events_and_tools(
            research_turn("request-approval"),
            cancel_rx,
            event_tx,
            Some(test_tool_session(handler.clone())),
        )
        .await
        .unwrap();

    assert!(handler.calls().await.is_empty());
    assert_eq!(outcome.status, "completed");
}

#[tokio::test]
async fn falls_back_safely_when_app_server_rejects_dynamic_tools() {
    let runtime = CodexRuntime::spawn(fake_command_without_dynamic_tools())
        .await
        .unwrap();
    let handler = Arc::new(RecordingHandler::default());
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let outcome = runtime
        .run_turn_with_events_and_tools(
            research_turn("ordinary fallback turn"),
            cancel_rx,
            event_tx,
            Some(test_tool_session(handler)),
        )
        .await
        .unwrap();

    assert_eq!(outcome.status, "completed");
    assert!(!runtime.capabilities().supports_dynamic_tools);
    let mut unavailable = false;
    while let Ok(event) = event_rx.try_recv() {
        unavailable |= event.kind == "dynamic-tools-unavailable";
    }
    assert!(unavailable);
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

#[cfg(target_os = "linux")]
#[tokio::test]
async fn rebuilds_project_local_runtime_tmp_before_each_turn() {
    let root = tempfile::tempdir().unwrap();
    let runtime_tmp = root.path().join(".runtime/tmp");
    let mut command = fake_command();
    command.runtime_tmp = Some(runtime_tmp.clone());
    let runtime = CodexRuntime::spawn(command).await.unwrap();

    tokio::fs::remove_dir_all(&runtime_tmp).await.unwrap();
    assert!(!runtime_tmp.exists());

    let (_cancel_tx, cancel_rx) = watch::channel(false);
    let reported_tmp = next_test_runtime_tmp(runtime.subscribe());
    runtime
        .run_turn(
            CodexTurn {
                thread_id: None,
                cwd: root.path().to_path_buf(),
                prompt: "runtime-tmp".into(),
                output_schema: None,
                settings: standard_settings(),
            },
            cancel_rx,
        )
        .await
        .unwrap();

    let reported_tmp = tokio::time::timeout(std::time::Duration::from_secs(1), reported_tmp)
        .await
        .unwrap();
    assert_eq!(reported_tmp, runtime_tmp);
    assert!(runtime_tmp.exists());
    assert!(runtime_tmp
        .join(format!("codex-bwrap-synthetic-mount-targets-{}", unsafe {
            libc::geteuid()
        }))
        .is_dir());
}
