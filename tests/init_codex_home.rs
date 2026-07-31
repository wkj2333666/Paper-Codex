use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/init-codex-home.sh"
    )
}

fn run_initializer(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new("bash")
        .arg(script())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
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
    fs::write(
        source.join("skills/private-writer/SKILL.md"),
        "personal",
    )
    .unwrap();

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

    let first = run_initializer(
        temp.path(),
        &["--target", target.to_str().unwrap()],
    );
    assert!(first.status.success());
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "cli_auth_credentials_store = \"file\"\n"
    );

    fs::write(target.join("config.toml"), "model = \"kept\"\n").unwrap();
    let second = run_initializer(
        temp.path(),
        &["--target", target.to_str().unwrap()],
    );
    assert!(second.status.success());
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "model = \"kept\"\n"
    );
}
