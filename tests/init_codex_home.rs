use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn script() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/init-codex-home.sh")
}

fn run_initializer(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new("bash")
        .arg(script())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn repository_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

#[test]
fn imports_config_and_personal_skills_without_auth_or_runtime_state() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    fs::create_dir_all(source.join("skills/.system/system-skill")).unwrap();
    fs::create_dir_all(source.join("skills/private-writer")).unwrap();
    fs::create_dir_all(source.join("sessions")).unwrap();
    fs::create_dir_all(source.join("packages")).unwrap();
    fs::write(
        source.join("config.toml"),
        "model = \"gpt-test\"\nsqlite_home = \"/shared/sqlite\"\ncli_auth_credentials_store = \"keyring\"\n\n[mcp_servers.ssh-bridge]\ncommand = \"bridge\"\n",
    )
    .unwrap();
    fs::write(source.join("auth.json"), "secret").unwrap();
    fs::write(source.join("sessions/thread.jsonl"), "thread").unwrap();
    fs::write(source.join("packages/blob"), "large-cache").unwrap();
    fs::write(
        source.join("skills/.system/system-skill/SKILL.md"),
        "system",
    )
    .unwrap();
    fs::write(source.join("skills/private-writer/SKILL.md"), "personal").unwrap();

    let output = run_initializer(
        temp.path(),
        &[
            "--import-from",
            source.to_str().unwrap(),
            "--target",
            target.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config = fs::read_to_string(target.join("config.toml")).unwrap();
    assert!(config.starts_with("cli_auth_credentials_store = \"file\"\n"));
    assert!(config.contains("model = \"gpt-test\""));
    assert!(config.contains("[mcp_servers.ssh-bridge]"));
    assert!(!config.contains("sqlite_home"));
    assert!(!config.contains("keyring"));
    assert!(target.join("skills/private-writer/SKILL.md").is_file());
    assert!(!target.join("skills/.system").exists());
    assert!(!target.join("auth.json").exists());
    assert!(!target.join("sessions").exists());
    assert!(!target.join("packages").exists());
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn initializes_empty_home_and_preserves_existing_config() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");

    let first = run_initializer(temp.path(), &["--target", target.to_str().unwrap()]);
    assert!(first.status.success());
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "cli_auth_credentials_store = \"file\"\n"
    );

    fs::write(
        target.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\nmodel = \"kept\"\n",
    )
    .unwrap();
    let second = run_initializer(temp.path(), &["--target", target.to_str().unwrap()]);
    assert!(second.status.success());
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "cli_auth_credentials_store = \"file\"\nmodel = \"kept\"\n"
    );
}

#[test]
fn rejects_existing_config_that_can_use_shared_keyring() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("config.toml"), "model = \"unsafe\"\n").unwrap();

    let output = run_initializer(temp.path(), &["--target", target.to_str().unwrap()]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = "existing config.toml must set cli_auth_credentials_store to file";
    assert!(stderr.contains(expected));
}

#[test]
fn user_service_cannot_write_main_codex_home() {
    let service = repository_file("deploy/paper-codex.user.service");
    assert!(service.contains(
        "ExecStartPre=/usr/bin/install -d -m 0700 %h/projects/paper-codex/.runtime/codex-home"
    ));
    assert!(!service.contains("InaccessiblePaths=-%h/.codex"));
    assert!(!service
        .lines()
        .any(|line| { line.starts_with("ReadWritePaths=") && line.contains("%h/.codex") }));
}

#[test]
fn user_service_allows_only_the_ssh_bridge_control_directory_in_tmp() {
    let service = repository_file("deploy/paper-codex.user.service");
    assert!(service.contains("PrivateTmp=false"));
    assert!(service.lines().any(|line| {
        line.starts_with("ReadWritePaths=") && line.contains("/tmp/codex-ssh-bridge-%U")
    }));
    assert!(!service.lines().any(|line| line == "ReadWritePaths=/tmp"));
}

#[test]
fn environment_examples_select_project_local_codex_home() {
    for path in [
        "paper-codex.env.example",
        "deploy/paper-codex.user.env.example",
    ] {
        let content = repository_file(path);
        assert!(content.contains("PAPER_CODEX_CODEX_HOME=./.runtime/codex-home"));
    }
}

#[test]
fn release_archive_includes_codex_home_initializer() {
    let workflow = repository_file(".github/workflows/release.yml");
    assert!(workflow
        .contains("install -m 0755 scripts/init-codex-home.sh \"${package}/init-codex-home.sh\""));
}
