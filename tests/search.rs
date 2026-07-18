use assert_cmd::Command;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn agent_history(home: &Path, fake_bin: &Path, argv_log: &Path, output: &str) -> Command {
    let mut command = Command::cargo_bin("agent-history").expect("agent-history binary");
    let mut path_entries = vec![fake_bin.to_path_buf()];
    path_entries.extend(env::split_paths(&env::var_os("PATH").expect("PATH")));

    command
        .env("HOME", home)
        .env("PATH", env::join_paths(path_entries).expect("PATH entries"))
        .env("FAKE_ARGV_LOG", argv_log)
        .env("FAKE_OUTPUT", output);
    command
}

fn fake_claude_memory(root: &Path) -> (PathBuf, PathBuf) {
    let bin = root.join("bin");
    let executable = bin.join("claude-memory");
    let argv_log = root.join("claude-memory.argv");
    fs::create_dir_all(&bin).expect("create fake backend directory");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s\\n' '---' >> \"$FAKE_ARGV_LOG\"\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\" >> \"$FAKE_ARGV_LOG\"; done\nprintf '%s' \"$FAKE_OUTPUT\"\nexit 0\n",
    )
    .expect("write fake backend");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&executable)
            .expect("fake backend metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("make fake backend executable");
    }

    (bin, argv_log)
}

fn invocations(argv_log: &Path) -> Vec<Vec<String>> {
    let content = fs::read_to_string(argv_log).expect("backend argv log");
    content
        .split("---\n")
        .filter(|invocation| !invocation.is_empty())
        .map(|invocation| invocation.lines().map(str::to_owned).collect())
        .collect()
}

fn assert_in_order(output: &str, values: &[&str]) {
    let mut offset = 0;
    for value in values {
        let position = output[offset..]
            .find(value)
            .unwrap_or_else(|| panic!("missing {value:?} after byte {offset} in {output:?}"));
        offset += position + value.len();
    }
}

#[test]
fn plain_query_invokes_claude_memory_once_with_default_limit() {
    let home = TempDir::new().expect("temp home");
    let fake_root = TempDir::new().expect("fake backend root");
    let (fake_bin, argv_log) = fake_claude_memory(fake_root.path());
    let output = r#"{"type":"answer","text":"backend result","source":"session","path":"/history/one","session_id":"session-one","score":0.99}
"#;

    agent_history(home.path(), &fake_bin, &argv_log, output)
        .args(["search", "literal .* [query]", "--no-color"])
        .assert()
        .success();

    assert_eq!(
        invocations(&argv_log),
        vec![vec![
            "search".to_owned(),
            "--json".to_owned(),
            "--limit".to_owned(),
            "5".to_owned(),
            "literal .* [query]".to_owned(),
        ]]
    );
}

#[test]
fn max_count_aliases_forward_their_values_as_backend_limit() {
    let home = TempDir::new().expect("temp home");
    let fake_root = TempDir::new().expect("fake backend root");
    let (fake_bin, argv_log) = fake_claude_memory(fake_root.path());
    let output = r#"{"type":"answer","text":"backend result","source":"session","path":"/history/one","session_id":"session-one","score":0.99}
"#;

    agent_history(home.path(), &fake_bin, &argv_log, output)
        .args(["search", "query", "-m", "3"])
        .assert()
        .success();
    agent_history(home.path(), &fake_bin, &argv_log, output)
        .args(["search", "query", "--max-count", "7"])
        .assert()
        .success();

    let calls = invocations(&argv_log);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][..4], ["search", "--json", "--limit", "3"]);
    assert_eq!(calls[1][..4], ["search", "--json", "--limit", "7"]);
}

#[test]
fn json_output_preserves_backend_records_and_rank_order() {
    let home = TempDir::new().expect("temp home");
    let fake_root = TempDir::new().expect("fake backend root");
    let (fake_bin, argv_log) = fake_claude_memory(fake_root.path());
    let backend_output = concat!(
        "{\"type\":\"answer\",\"text\":\"first result\",\"source\":\"session\",\"path\":\"/history/first\",\"session_id\":\"session-first\",\"score\":0.91}\n",
        "{\"type\":\"prompt\",\"text\":\"second result\",\"source\":\"archive\",\"path\":\"/history/second\",\"session_id\":\"session-second\",\"score\":0.42}\n",
    );

    let output = agent_history(home.path(), &fake_bin, &argv_log, backend_output)
        .args(["search", "query", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(output, backend_output.as_bytes());
}

#[test]
fn human_output_renders_score_type_session_path_then_text_without_color() {
    let home = TempDir::new().expect("temp home");
    let fake_root = TempDir::new().expect("fake backend root");
    let (fake_bin, argv_log) = fake_claude_memory(fake_root.path());
    let backend_output = concat!(
        "{\"type\":\"answer\",\"text\":\"first human result\",\"source\":\"session\",\"path\":\"/history/first\",\"session_id\":\"session-first\",\"score\":0.91}\n",
        "{\"type\":\"prompt\",\"text\":\"second human result\",\"source\":\"archive\",\"path\":\"/history/second\",\"session_id\":\"session-second\",\"score\":0.42}\n",
    );

    let output = String::from_utf8(
        agent_history(home.path(), &fake_bin, &argv_log, backend_output)
            .args(["search", "query", "--no-color"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .expect("human output UTF-8");

    assert_in_order(
        &output,
        &[
            "0.91",
            "answer",
            "session-first",
            "/history/first",
            "first human result",
            "0.42",
            "prompt",
            "session-second",
            "/history/second",
            "second human result",
        ],
    );
    assert!(
        !output.contains('\x1b'),
        "unexpected ANSI escape: {output:?}"
    );
}

#[test]
fn captured_human_output_without_no_color_has_no_ansi_escapes() {
    let home = TempDir::new().expect("temp home");
    let fake_root = TempDir::new().expect("fake backend root");
    let (fake_bin, argv_log) = fake_claude_memory(fake_root.path());
    let backend_output = "{\"type\":\"answer\",\"text\":\"captured human result\",\"source\":\"session\",\"path\":\"/history/captured\",\"session_id\":\"session-captured\",\"score\":0.73}\n";

    let output = agent_history(home.path(), &fake_bin, &argv_log, backend_output)
        .args(["search", "query"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert!(
        !output.contains(&b'\x1b'),
        "unexpected ANSI escape in captured human output: {output:?}"
    );
}

#[test]
fn removed_options_are_rejected_by_clap() {
    let home = TempDir::new().expect("temp home");
    let fake_root = TempDir::new().expect("fake backend root");
    let (fake_bin, argv_log) = fake_claude_memory(fake_root.path());
    let removed_options = [
        vec!["--source", "claude"],
        vec!["--role", "assistant"],
        vec!["--project", "/tmp/project"],
        vec!["--session", "session"],
        vec!["--since", "2026-01-01"],
        vec!["--until", "2026-01-01"],
        vec!["-i"],
        vec!["--ignore-case"],
        vec!["-l"],
        vec!["--files-with-matches"],
        vec!["--live-only"],
        vec!["--archive-only"],
        vec!["--all"],
    ];

    for option in removed_options {
        let mut args = vec!["search", "query"];
        args.extend(option);
        agent_history(home.path(), &fake_bin, &argv_log, "")
            .args(args)
            .assert()
            .failure()
            .stderr(predicates::str::contains("unexpected argument"));
    }
}
