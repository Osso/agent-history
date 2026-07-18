use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn agent_history(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("agent-history").expect("agent-history binary");
    cmd.env("HOME", home);
    cmd
}

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("parent dir")).expect("create parent dir");
    fs::write(path, content).expect("write fixture");
}

fn write_claude_session(home: &Path) {
    write_file(
        &home.join(".claude/projects/-tmp-demo/claude-abc123.jsonl"),
        r#"{"type":"user","sessionId":"claude-abc123","timestamp":"2026-06-01T10:00:00Z","cwd":"/tmp/demo","message":{"content":"Need alpha search"}}
{"type":"assistant","sessionId":"claude-abc123","timestamp":"2026-06-01T10:01:00Z","cwd":"/tmp/demo","message":{"content":[{"type":"text","text":"Visible beta reply"},{"type":"tool_use","input":{"query":"hidden gamma claude-tool"}},{"type":"tool_result","content":"hidden gamma claude-result"}]}}
"#,
    );
}

fn write_codex_session(home: &Path) {
    write_file(
        &home.join(".codex/sessions/2026/06/02/rollout-2026-06-02T11-00-00-codex-live.jsonl"),
        r#"{"type":"session_meta","payload":{"id":"codex-live","cwd":"/tmp/demo"}}
{"type":"response_item","timestamp":"2026-06-02T11:00:00Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Codex alpha request"}]}}
{"type":"response_item","timestamp":"2026-06-02T11:01:00Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Codex beta answer"}]}}
{"type":"response_item","timestamp":"2026-06-02T11:02:00Z","payload":{"type":"function_call","name":"read","arguments":"hidden gamma codex-tool"}}
{"type":"response_item","timestamp":"2026-06-02T11:03:00Z","payload":{"type":"function_call_output","output":"hidden gamma codex-output"}}
"#,
    );
}

fn write_pi_session(home: &Path) {
    write_file(
        &home.join(".config/pi/agent/sessions/demo-project/2026-06-03T12-00-00-000Z_pi-live.jsonl"),
        r#"{"type":"session","version":3,"id":"pi-live","timestamp":"2026-06-03T12:00:00Z","cwd":"/tmp/demo"}
{"type":"message","id":"m1","timestamp":"2026-06-03T12:00:01Z","message":{"role":"user","content":[{"type":"text","text":"Pi alpha prompt"}]}}
{"type":"message","id":"m2","timestamp":"2026-06-03T12:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"Pi beta response"},{"type":"toolCall","input":{"query":"hidden gamma pi-toolcall"}},{"type":"toolResult","content":"hidden gamma pi-toolresult"},{"type":"tool_result","content":"hidden gamma pi-tool-result"}]}}
"#,
    );
}

#[test]
fn searches_all_sources_by_default() {
    let tmp = TempDir::new().expect("temp dir");
    write_claude_session(tmp.path());
    write_codex_session(tmp.path());
    write_pi_session(tmp.path());

    agent_history(tmp.path())
        .args(["search", "alpha", "--no-color"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude:claude-abc123"))
        .stdout(predicate::str::contains("codex:codex-live"))
        .stdout(predicate::str::contains("pi:pi-live"));
}

#[test]
fn filters_by_source_role_project_session_and_dates() {
    let tmp = TempDir::new().expect("temp dir");
    write_claude_session(tmp.path());
    write_codex_session(tmp.path());
    write_pi_session(tmp.path());

    agent_history(tmp.path())
        .args([
            "search",
            "beta",
            "--source",
            "codex",
            "--role",
            "assistant",
            "--project",
            "/tmp/demo",
            "--session",
            "codex-live",
            "--since",
            "2026-06-02",
            "--until",
            "2026-06-02",
            "--no-color",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("codex:codex-live"))
        .stdout(predicate::str::contains("Codex beta answer"))
        .stdout(predicate::str::contains("claude:").not())
        .stdout(predicate::str::contains("pi:").not());
}

#[test]
fn hidden_tool_payloads_require_all_flag() {
    let tmp = TempDir::new().expect("temp dir");
    write_claude_session(tmp.path());
    write_codex_session(tmp.path());
    write_pi_session(tmp.path());

    agent_history(tmp.path())
        .args(["search", "hidden gamma", "--no-color"])
        .assert()
        .failure()
        .code(1);

    agent_history(tmp.path())
        .args(["search", "hidden gamma", "--all", "--no-color"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hidden gamma"));
}

#[test]
fn files_with_matches_deduplicates_session_paths_and_honors_max_count() {
    let tmp = TempDir::new().expect("temp dir");
    write_claude_session(tmp.path());
    write_codex_session(tmp.path());
    write_pi_session(tmp.path());

    agent_history(tmp.path())
        .args(["search", "beta|alpha", "-l", "-m", "2", "--no-color"])
        .assert()
        .success()
        .stdout(predicate::function(|stdout: &str| {
            let lines = stdout.lines().collect::<Vec<_>>();
            lines.len() == 2
                && lines[0].contains("claude-abc123.jsonl")
                && lines[1].contains("rollout-2026-06-02T11-00-00-codex-live.jsonl")
        }));
}

#[test]
fn json_output_does_not_serialize_raw_or_hidden_payloads_by_default() {
    let tmp = TempDir::new().expect("temp dir");
    write_pi_session(tmp.path());

    let output = agent_history(tmp.path())
        .args([
            "search",
            "Pi alpha",
            "--source",
            "pi",
            "--json",
            "--no-color",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).expect("valid json line");

    assert_eq!(json["source"], "pi");
    assert_eq!(json["session"], "pi-live");
    assert_eq!(json["role"], "user");
    assert_eq!(json["cwd"], "/tmp/demo");
    assert_eq!(json["text"], "Pi alpha prompt");
    assert!(json.get("raw").is_none());

    let hidden_output = agent_history(tmp.path())
        .args([
            "search",
            "Pi beta",
            "--source",
            "pi",
            "--json",
            "--no-color",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let hidden_json: serde_json::Value =
        serde_json::from_slice(&hidden_output).expect("valid json line");

    assert!(hidden_json.get("raw").is_none());
    assert!(
        !hidden_output
            .windows("hidden gamma".len())
            .any(|window| window == b"hidden gamma")
    );
}

#[test]
fn role_tool_matches_hidden_tool_payloads_across_sources_when_all_is_set() {
    let tmp = TempDir::new().expect("temp dir");
    write_claude_session(tmp.path());
    write_codex_session(tmp.path());
    write_pi_session(tmp.path());

    agent_history(tmp.path())
        .args([
            "search",
            "hidden gamma",
            "--all",
            "--role",
            "tool",
            "--no-color",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-tool"))
        .stdout(predicate::str::contains("claude-result"))
        .stdout(predicate::str::contains("codex-tool"))
        .stdout(predicate::str::contains("codex-output"))
        .stdout(predicate::str::contains("pi-toolcall"))
        .stdout(predicate::str::contains("pi-toolresult"))
        .stdout(predicate::str::contains("pi-tool-result"))
        .stdout(predicate::str::contains(" tool "));
}

#[test]
fn pi_source_without_storage_returns_no_results_not_an_error() {
    let tmp = TempDir::new().expect("temp dir");

    agent_history(tmp.path())
        .args(["search", "anything", "--source", "pi", "--no-color"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::is_empty());
}

#[test]
fn claude_slash_project_filter_keeps_matching_session() {
    let tmp = TempDir::new().expect("temp dir");
    write_file(
        &tmp.path()
            .join(".claude/projects/-tmp-globalcomix-gc/claude-slash.jsonl"),
        r#"{"type":"user","sessionId":"claude-slash","timestamp":"2026-06-04T10:00:00Z","cwd":"/tmp/globalcomix/gc","message":{"content":"Claude slash project"}}
"#,
    );

    agent_history(tmp.path())
        .args([
            "search",
            "Claude slash project",
            "--source",
            "claude",
            "--project",
            "globalcomix/gc",
            "--no-color",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude:claude-slash"))
        .stdout(predicate::str::contains("Claude slash project"));
}

#[test]
fn pi_slash_project_filter_keeps_matching_session() {
    let tmp = TempDir::new().expect("temp dir");
    write_file(
        &tmp.path()
            .join(".config/pi/agent/sessions/demo-project/pi-slash.jsonl"),
        r#"{"type":"session","version":3,"id":"pi-slash","timestamp":"2026-06-05T10:00:00Z","cwd":"/tmp/pi/project"}
{"type":"message","id":"m1","timestamp":"2026-06-05T10:00:01Z","message":{"role":"user","content":[{"type":"text","text":"Pi slash project"}]}}
"#,
    );

    agent_history(tmp.path())
        .args([
            "search",
            "Pi slash project",
            "--source",
            "pi",
            "--project",
            "/tmp/pi/project",
            "--no-color",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pi:pi-slash"))
        .stdout(predicate::str::contains("Pi slash project"));
}

#[test]
fn claude_hyphen_project_filter_keeps_matching_session() {
    let tmp = TempDir::new().expect("temp dir");
    write_file(
        &tmp.path()
            .join(".claude/projects/-tmp-foo-bar/claude-hyphen.jsonl"),
        r#"{"type":"user","sessionId":"claude-hyphen","timestamp":"2026-06-06T10:00:00Z","cwd":"/tmp/foo-bar","message":{"content":"Claude hyphen project"}}
"#,
    );

    agent_history(tmp.path())
        .args([
            "search",
            "Claude hyphen project",
            "--source",
            "claude",
            "--project",
            "/tmp/foo-bar",
            "--no-color",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude:claude-hyphen"))
        .stdout(predicate::str::contains("Claude hyphen project"));
}
