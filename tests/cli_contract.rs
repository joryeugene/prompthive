use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

struct Fixture {
    temporary: TempDir,
    store: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = TempDir::new().unwrap();
        let store = temporary.path().join("store");
        fs::create_dir(&store).unwrap();
        Self { temporary, store }
    }

    fn path(&self, id: &str) -> PathBuf {
        if let Some(team) = id.strip_prefix('@') {
            self.store.join("teams").join(format!("{team}.md"))
        } else if id.contains('/') {
            self.store.join("banks").join(format!("{id}.md"))
        } else {
            self.store.join("prompts").join(format!("{id}.md"))
        }
    }

    fn raw(&self, id: &str, content: &str) {
        let path = self.path(id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn prompt(&self, id: &str, body: &str) {
        let yaml_id = serde_yaml::to_string(id).unwrap();
        self.raw(
            id,
            &format!(
                "---\nid: {}\ndescription: Fixture\n---\n\n{body}",
                yaml_id.trim_end()
            ),
        );
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ph"));
        command
            .current_dir(self.temporary.path())
            .env("PROMPTHIVE_BASE_DIR", &self.store)
            .env("XDG_CONFIG_HOME", self.temporary.path().join("config"))
            .env("XDG_RUNTIME_DIR", self.temporary.path().join("runtime"))
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("RUST_BACKTRACE", "0")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("PROMPTHIVE_TEST_MODE")
            .env_remove("PROMPTHIVE_CONTRACT_SECRET")
            .env_remove("PROMPTHIVE_CONTRACT_ABSENT_7842")
            .env_remove("EDITOR")
            .env_remove("VISUAL")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, args: &[&str], input: &[u8]) -> Output {
        self.run_env(args, input, &[])
    }

    fn run_env(&self, args: &[&str], input: &[u8], env: &[(&str, &str)]) -> Output {
        let mut command = self.command();
        command.args(args);
        for (name, value) in env {
            command.env(name, value);
        }
        let mut child = command.spawn().unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let stdout_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let stderr_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).unwrap();
            bytes
        });
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(input) {
                assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                timed_out = true;
                break child.wait().unwrap();
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_reader.join().unwrap();
        let stderr = stderr_reader.join().unwrap();
        assert!(
            !timed_out,
            "ph {args:?} hung: {}",
            String::from_utf8_lossy(&stderr)
        );
        Output {
            status,
            stdout,
            stderr,
        }
    }

    fn success(&self, args: &[&str], input: &[u8]) -> Output {
        let output = self.run(args, input);
        assert!(
            output.status.success(),
            "ph {args:?}: {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn fail(&self, args: &[&str], input: &[u8], code: i32) -> Output {
        let output = self.run(args, input);
        assert_eq!(
            output.status.code(),
            Some(code),
            "ph {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stdout.is_empty(),
            "failure leaked payload: ph {args:?}"
        );
        assert!(!output.stderr.is_empty(), "failure has no recovery message");
        output
    }
}

fn files_under(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !directory.exists() {
            return;
        }
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                visit(root, &entry.path(), found);
            } else if kind.is_file() {
                found.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut found = BTreeMap::new();
    visit(root, root, &mut found);
    found
}

#[test]
fn short_and_long_action_help_keep_the_same_command() {
    let fixture = Fixture::new();
    for (short, long) in [
        ("u", "use"),
        ("f", "find"),
        ("n", "new"),
        ("e", "edit"),
        ("c", "compose"),
        ("s", "show"),
        ("l", "ls"),
        ("list", "ls"),
        ("r", "rename"),
        ("mv", "rename"),
        ("d", "delete"),
        ("rm", "delete"),
        ("x", "clean"),
        ("t", "tui"),
        ("chain", "compose"),
    ] {
        let short_help = fixture.success(&[short, "--help"], b"");
        let long_help = fixture.success(&[long, "--help"], b"");
        assert_eq!(short_help.stdout, long_help.stdout, "alias {short}");
    }
}

#[test]
fn version_flags_agree_with_the_package_version() {
    let fixture = Fixture::new();
    for flag in ["-v", "-V", "--version"] {
        let output = fixture.success(&[flag], b"");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            format!("ph {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn help_has_useful_local_actions_without_creating_a_store() {
    let fixture = Fixture::new();
    fs::remove_dir(&fixture.store).unwrap();
    let output = fixture.success(&[], b"");
    let help = String::from_utf8(output.stdout).unwrap();
    for command in ["use", "find", "new", "edit", "compose"] {
        assert!(help.contains(command), "missing {command} from {help}");
    }
    assert!(
        help.lines().count() < 20,
        "bare ph help is not compact: {help}"
    );
    assert!(help.contains("ph n review"));
    assert!(help.contains("git diff | ph u review"));
    assert!(!help.contains("REGISTRY & SHARING"));
    assert!(!help.contains("CLOUD & TEAMS"));
    assert!(!fixture.store.exists(), "plain help created a prompt store");
}

#[test]
fn headless_context_requires_a_visible_selection_or_exact_name() {
    let fixture = Fixture::new();
    fixture.fail(&[], b"context", 2);
    fixture.fail(&["u"], b"context", 2);
    let output = fixture.run(&["t"], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn there_is_no_bare_prompt_name_fallback() {
    let fixture = Fixture::new();
    fixture.prompt("rv", "This is data, not a command");
    fixture.fail(&["rv"], b"", 2);
    assert_eq!(
        fixture.success(&["u", "rv"], b"").stdout,
        b"This is data, not a command"
    );
}

#[test]
fn new_preserves_literal_argument_and_stdin_content() {
    let fixture = Fixture::new();
    let body = "  Review {input}\n\n";
    fixture.success(&["n", "notes/first", body, "-q"], b"");
    fixture.success(&["new", body, "notes/reversed", "-q"], b"");
    fixture.success(&["n", "notes/piped", "-q"], body.as_bytes());
    for id in ["notes/first", "notes/reversed", "notes/piped"] {
        assert_eq!(
            fixture.success(&["s", id, "-q"], b"").stdout,
            body.as_bytes()
        );
    }
    fixture.fail(&["n", "Two content words", "More content words"], b"", 2);
    fixture.fail(&["n", "notes/first", "Replacement content"], b"", 2);
    assert_eq!(
        fixture.success(&["s", "notes/first"], b"").stdout,
        body.as_bytes()
    );
}

#[test]
fn invalid_new_name_is_not_reversed_into_a_payload_prompt() {
    let fixture = Fixture::new();
    for name in ["../escaped", "bank/../escaped", "/absolute"] {
        fixture.fail(&["n", name, "payload"], b"", 2);
        assert!(
            !fixture.path("payload").exists(),
            "invalid NAME was treated as content"
        );
    }
    assert!(!fixture.temporary.path().join("escaped.md").exists());
}

#[test]
fn new_explicit_file_destination_never_also_saves_a_prompt() {
    let fixture = Fixture::new();
    let existing = fixture.temporary.path().join("existing.txt");
    fs::write(&existing, b"Existing file").unwrap();
    let output = fixture.run(&["n", "one", "text", "-f", existing.to_str().unwrap()], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(existing).unwrap(), b"Existing file");
    assert!(!fixture.path("one").exists());
    let fresh = fixture.temporary.path().join("new.txt");
    let output = fixture.success(&["n", "three", "text", "-f", fresh.to_str().unwrap()], b"");
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(fresh).unwrap(), b"text");
    assert!(!fixture.path("three").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn failed_new_copy_does_not_save_an_implicit_prompt() {
    let fixture = Fixture::new();
    fixture.fail(&["n", "two", "text", "-c"], b"", 4);
    assert!(!fixture.path("two").exists());
}

#[test]
fn exact_resolution_is_shared_and_scripts_never_fuzzy_select() {
    let fixture = Fixture::new();
    fixture.prompt("auth/basic", "Basic {input}");
    fixture.prompt("auth/bearer", "Bearer {input}");
    assert_eq!(
        fixture.success(&["u", "auth/basic", "request"], b"").stdout,
        b"Basic request"
    );
    assert_eq!(
        fixture.success(&["s", "auth/basic"], b"").stdout,
        b"Basic {input}"
    );
    assert_eq!(
        fixture.success(&["c", "auth/basic", "request"], b"").stdout,
        b"Basic request"
    );
    for query in ["auth/bas", "auth/b", "bas"] {
        let output = fixture.fail(&["u", query], b"", 2);
        assert!(String::from_utf8_lossy(&output.stderr).contains("auth/basic"));
    }
    fixture.fail(&["d", "auth/bas"], b"", 2);
    fixture.fail(&["r", "auth/bas", "auth/renamed"], b"", 2);
    assert!(fixture.path("auth/basic").exists());
}

#[test]
fn canonical_names_win_over_verified_compatibility_codes() {
    let fixture = Fixture::new();
    fixture.prompt("essentials/commit", "Commit body");
    let binding = "version = 1\nverified = true\n[bindings]\ne = \"essentials/commit\"\n";
    fs::write(fixture.store.join("compatibility.toml"), binding).unwrap();
    assert_eq!(fixture.success(&["u", "e"], b"").stdout, b"Commit body");
    fixture.prompt("email/other", "An unrelated addition");
    assert_eq!(fixture.success(&["u", "e"], b"").stdout, b"Commit body");
    fixture.prompt("e", "Exact root prompt");
    let output = fixture.success(&["u", "e"], b"");
    assert_eq!(output.stdout, b"Exact root prompt");
    let error = String::from_utf8_lossy(&output.stderr).to_lowercase();
    assert!(error.contains("binding") || error.contains("compatibility"));
    assert_eq!(
        fs::read_to_string(fixture.store.join("compatibility.toml")).unwrap(),
        binding
    );
    fixture.fail(&["e"], b"", 2);
}

#[test]
fn unverified_codes_are_not_generated_or_adopted_during_render() {
    let fixture = Fixture::new();
    fixture.prompt("essentials/commit", "Commit body");
    fixture.fail(&["u", "e"], b"", 2);
    let binding = "version = 1\nverified = false\n[bindings]\ne = \"essentials/commit\"\n";
    fs::write(fixture.store.join("compatibility.toml"), binding).unwrap();
    fixture.fail(&["u", "e"], b"", 2);
    assert_eq!(
        fs::read_to_string(fixture.store.join("compatibility.toml")).unwrap(),
        binding
    );
}

#[test]
fn malformed_compatibility_cannot_hide_exact_names_or_disclose_values() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Exact canonical text");
    let private = "synthetic-private-token-85642";
    let config = format!("version = 1\nverified = true\n[bindings]\ne = [{private}\n");
    fs::write(fixture.store.join("compatibility.toml"), &config).unwrap();
    let exact = fixture.success(&["u", "review"], b"");
    assert_eq!(exact.stdout, b"Exact canonical text");
    assert!(!String::from_utf8_lossy(&exact.stderr).contains(private));
    let missing = fixture.run(&["u", "e"], b"");
    assert!(!missing.status.success());
    assert!(missing.stdout.is_empty());
    assert!(!missing.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&missing.stderr).contains(private));
    assert_eq!(
        fs::read_to_string(fixture.store.join("compatibility.toml")).unwrap(),
        config
    );
}

#[test]
fn malformed_retained_editor_config_reports_no_private_source_values() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Exact canonical text");
    let original = fs::read(fixture.path("review")).unwrap();
    let private = "synthetic-private-editor-token-64831";
    let config = format!("private_token = [{private}\n");
    fs::write(fixture.store.join("config.toml"), &config).unwrap();
    assert_eq!(
        fixture.success(&["u", "review"], b"").stdout,
        b"Exact canonical text"
    );
    // Fixture::command removes EDITOR and VISUAL, forcing the retained config path.
    let output = fixture.run(&["e", "review"], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(!diagnostic.is_empty());
    assert!(diagnostic.contains("config.toml"));
    assert!(!diagnostic.contains(private));
    assert_eq!(fs::read(fixture.path("review")).unwrap(), original);
    assert_eq!(
        fs::read_to_string(fixture.store.join("config.toml")).unwrap(),
        config
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_canonical_collision_cannot_redirect_through_a_binding() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.prompt("work/review", "Wrong redirected prompt");
    let outside = fixture.temporary.path().join("external.md");
    fs::write(&outside, b"Outside canonical content").unwrap();
    fs::create_dir_all(fixture.store.join("prompts")).unwrap();
    symlink(&outside, fixture.path("e")).unwrap();
    fs::write(
        fixture.store.join("compatibility.toml"),
        "version=1\nverified=true\n[bindings]\ne=\"work/review\"\n",
    )
    .unwrap();
    let output = fixture.run(&["u", "e"], b"");
    assert!(
        !output.status.success(),
        "unsafe canonical path fell through to a binding"
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(outside).unwrap(), b"Outside canonical content");
}

#[test]
fn input_is_literal_and_whitespace_is_exact() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Review [{input}]");
    let input = "  {env:PROMPTHIVE_CONTRACT_SECRET} {date} $(echo ignored)\n\n";
    let output = fixture.run_env(
        &["u", "review"],
        input.as_bytes(),
        &[("PROMPTHIVE_CONTRACT_SECRET", "synthetic-only")],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, format!("Review [{input}]").as_bytes());
    fixture.fail(&["u", "review", "argument"], b"piped", 2);
    fixture.fail(&["u", "review"], &[0xff, 0xfe], 2);
}

#[test]
fn unicode_use_does_not_panic_or_write_content_history() {
    let fixture = Fixture::new();
    fixture.prompt("review", "{input}");
    let old_history = b"Old user history must remain untouched";
    fs::write(fixture.store.join("history.json"), old_history).unwrap();
    for input in [
        format!("{}😀", "a".repeat(99)),
        "😀".to_owned(),
        "a".repeat(120),
    ] {
        let output = fixture.success(&["u", "review", &input], b"");
        assert_eq!(output.stdout, input.as_bytes());
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        fs::read(fixture.store.join("history.json")).unwrap(),
        old_history
    );
}

#[test]
fn unicode_and_case_sensitive_canonical_names_stay_distinct() {
    let fixture = Fixture::new();
    fixture.success(&["n", "notes/解释", "解释这段文字", "-q"], b"");
    fixture.prompt("Review", "Upper case");
    assert_eq!(
        fixture.success(&["u", "notes/解释"], b"").stdout,
        "解释这段文字".as_bytes()
    );
    assert_eq!(fixture.success(&["u", "Review"], b"").stdout, b"Upper case");
    if fixture.path("review").exists() {
        // This filesystem aliases case-only names. Prove that the CLI does not
        // silently select that alias, or overwrite the existing prompt.
        assert_eq!(
            fs::read(fixture.path("review")).unwrap(),
            fs::read(fixture.path("Review")).unwrap()
        );
        fixture.fail(&["u", "review"], b"", 2);
        let refusal = fixture.run(&["n", "review", "Lower case", "-q"], b"");
        assert!(!refusal.status.success());
        assert_eq!(fixture.success(&["u", "Review"], b"").stdout, b"Upper case");
    } else {
        fixture.prompt("review", "Lower case");
        assert_eq!(fixture.success(&["u", "review"], b"").stdout, b"Lower case");
        assert_eq!(fixture.success(&["u", "Review"], b"").stdout, b"Upper case");
    }
    fixture.fail(&["u", "REVIEW"], b"", 2);
}

#[test]
fn every_legacy_input_slot_preserves_multiline_input() {
    let fixture = Fixture::new();
    let body = " {input}|{INPUT}|{content}|{CONTENT}\n\n";
    fixture.prompt("slots", body);
    let input = "line one\r\nline two\n\n";
    assert_eq!(
        fixture
            .success(&["u", "slots", "-q"], input.as_bytes())
            .stdout,
        format!(" {input}|{input}|{input}|{input}\n\n").as_bytes()
    );
    assert_eq!(
        fixture.success(&["s", "slots"], b"").stdout,
        body.as_bytes()
    );
}

#[test]
fn declared_variables_defaults_escapes_and_code_remain_distinct() {
    let fixture = Fixture::new();
    fixture.raw("vars", "---\nid: vars\ndescription: Variables\nvars: {lang: null, tone: brief}\n---\n\n{lang}|{tone}|{{lang}}|{foo}|{input}");
    let output = fixture.success(
        &["u", "vars", "VALUE {lang}", "-v", "lang=Rust=stable"],
        b"",
    );
    assert_eq!(
        output.stdout,
        b"Rust=stable|brief|{lang}|{foo}|VALUE {lang}"
    );
    let destination = fixture.temporary.path().join("untouched.md");
    fs::write(&destination, b"prior-file").unwrap();
    for arguments in [
        vec!["u", "vars", "-f", destination.to_str().unwrap(), "--force"],
        vec![
            "u",
            "vars",
            "-v",
            "lang=Rust",
            "-v",
            "unknown=value",
            "-f",
            destination.to_str().unwrap(),
            "--force",
        ],
        vec![
            "u",
            "vars",
            "-v",
            "lang=Rust",
            "-v",
            "lang=Go",
            "-f",
            destination.to_str().unwrap(),
            "--force",
        ],
    ] {
        fixture.fail(&arguments, b"", 2);
        assert_eq!(fs::read(&destination).unwrap(), b"prior-file");
    }
}

#[test]
fn explicit_environment_is_required_but_dotenv_is_not_loaded() {
    let fixture = Fixture::new();
    fixture.prompt("environment", "Value {env:PROMPTHIVE_CONTRACT_ABSENT_7842}");
    fs::write(
        fixture.temporary.path().join(".env"),
        "PROMPTHIVE_CONTRACT_ABSENT_7842=must-not-load\n",
    )
    .unwrap();
    fixture.fail(&["u", "environment"], b"", 2);
    let output = fixture.run_env(
        &["u", "environment"],
        b"",
        &[("PROMPTHIVE_CONTRACT_ABSENT_7842", "explicit-value")],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Value explicit-value");
}

#[test]
fn composition_is_ordered_literal_and_has_clean_stdout() {
    let fixture = Fixture::new();
    fixture.prompt("a", "A {input}");
    fixture.prompt("b", "B {content}");
    let output = fixture.success(&["c", "a,b"], b"context {env:NO_REPARSE}");
    assert_eq!(output.stdout, b"B A context {env:NO_REPARSE}");
    assert!(output.stderr.is_empty());
    fixture.prompt("dated", "Today {date}");
    let output = fixture.success(&["c", "a,dated", "context"], b"");
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(body.starts_with("Today "));
    assert!(body.ends_with("\n\nA context"));
    fixture.prompt("token-a", "{uuid}|{input}");
    fixture.prompt("token-b", "{uuid}|{input}");
    let output = fixture.success(&["c", "token-a,token-b", "context"], b"");
    let body = String::from_utf8(output.stdout).unwrap();
    let parts: Vec<_> = body.split('|').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], parts[1], "context token changed between stages");
    assert_eq!(parts[2], "context");
}

#[test]
fn composition_preflights_every_stage_and_variable() {
    let fixture = Fixture::new();
    fixture.raw(
        "a",
        "---\nid: a\ndescription: A\nvars: {word: null}\n---\n\n{word}",
    );
    fixture.raw(
        "b",
        "---\nid: b\ndescription: B\nvars: {tone: brief}\n---\n\n{tone}/{input}",
    );
    assert_eq!(
        fixture
            .success(&["c", "a,b", "-v", "word=Rust", "-v", "tone=terse"], b"")
            .stdout,
        b"terse/Rust"
    );
    let path = fixture.temporary.path().join("output.md");
    fs::write(&path, b"previous").unwrap();
    fixture.fail(
        &[
            "c",
            "a,missing",
            "-v",
            "word=Rust",
            "-f",
            path.to_str().unwrap(),
            "--force",
        ],
        b"",
        2,
    );
    assert_eq!(fs::read(path).unwrap(), b"previous");
}

#[test]
fn conflicting_composition_defaults_stay_local_and_supplied_values_are_literal() {
    let fixture = Fixture::new();
    fixture.raw(
        "a",
        "---\nid: a\nvars: {tone: gentle}\n---\n\n{tone}:{input}",
    );
    fixture.raw(
        "b",
        "---\nid: b\nvars: {tone: direct}\n---\n\n{tone}:{input}",
    );
    assert_eq!(
        fixture.success(&["c", "a,b", "context"], b"").stdout,
        b"direct:gentle:context"
    );
    let value = "literal {tone} {env:NOT_REQUESTED} = value";
    assert_eq!(
        fixture
            .success(
                &["c", "a,b", "context", "-v", &format!("tone={value}")],
                b""
            )
            .stdout,
        format!("{value}:{value}:context").as_bytes()
    );
    fixture.fail(&["c", "a,b", "-v", "unknown=bad"], b"", 2);
}

#[test]
fn dotted_prompt_ids_and_explicit_local_files_are_not_guessed() {
    let fixture = Fixture::new();
    fixture.prompt("review.v1", "Stored {input}");
    fs::write(
        fixture.temporary.path().join("review.v1"),
        "Wrong local file",
    )
    .unwrap();
    fs::write(fixture.temporary.path().join("direct.md"), "File {input}").unwrap();
    assert_eq!(
        fixture.success(&["c", "review.v1", "context"], b"").stdout,
        b"Stored context"
    );
    assert_eq!(
        fixture
            .success(&["c", "./direct.md", "context"], b"")
            .stdout,
        b"File context"
    );
}

#[test]
fn show_saves_the_unrendered_body_and_preserves_append_metadata() {
    let fixture = Fixture::new();
    fixture.prompt("source", "Original {input}");
    fixture.success(&["s", "source", "-s", "notes/copy"], b"");
    assert_eq!(
        fixture.success(&["s", "notes/copy"], b"").stdout,
        b"Original {input}"
    );
    fixture.fail(&["s", "source", "-s", "notes/copy"], b"", 2);
    fixture.raw(
        "target",
        "---\nid: target\ndescription: Target\ncustom: {keep: true}\n---\n\nFirst",
    );
    fixture.success(&["s", "source", "-a", "target"], b"");
    assert_eq!(
        fixture.success(&["s", "target"], b"").stdout,
        b"First\n\nOriginal {input}"
    );
    assert!(fs::read_to_string(fixture.path("target"))
        .unwrap()
        .contains("keep: true"));
}

#[test]
fn file_export_is_one_exact_destination_and_preserves_a_preimage() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Review {input}");
    let path = fixture.temporary.path().join("output.md");
    let output = fixture.success(
        &["u", "review", "context", "-f", path.to_str().unwrap()],
        b"",
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"Review context");
    assert!(!String::from_utf8_lossy(&output.stderr)
        .to_lowercase()
        .contains("sync"));
    let refusal = fixture.run(&["u", "review", "next", "-f", path.to_str().unwrap()], b"");
    assert!(
        !refusal.status.success(),
        "existing file replacement reported success"
    );
    assert!(refusal.stdout.is_empty());
    assert!(!refusal.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"Review context");
    fs::write(&path, b"unique-export-preimage-72645").unwrap();
    let output = fixture.success(
        &[
            "u",
            "review",
            "next",
            "-f",
            path.to_str().unwrap(),
            "--force",
            "-q",
        ],
        b"",
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"Review next");
    assert!(files_under(fixture.temporary.path())
        .values()
        .any(|bytes| bytes == b"unique-export-preimage-72645"));
}

#[test]
fn explicit_destinations_do_not_fall_back_to_pipe_payload() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Review {input}");
    let path = fixture.temporary.path().join("never-written.md");
    fixture.fail(&["u", "review", "-c", "-f", path.to_str().unwrap()], b"", 2);
    assert!(!path.exists());
    #[cfg(target_os = "linux")]
    for args in [
        vec!["u", "review", "-c"],
        vec!["u", "review", "-c", "-q"],
        vec!["c", "review", "--clipboard"],
    ] {
        let output = fixture.fail(&args, b"context", 4);
        assert!(String::from_utf8_lossy(&output.stderr)
            .to_lowercase()
            .contains("clipboard"));
    }
    fixture.fail(&["c", "review", "-c"], b"", 2);
}

#[test]
fn file_destination_failure_preserves_other_files_and_emits_no_payload() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Review {input}");
    let blocker = fixture.temporary.path().join("not-a-directory");
    fs::write(&blocker, b"keep-blocker").unwrap();
    let path = blocker.join("out.md");
    let output = fixture.run(
        &["u", "review", "context", "-f", path.to_str().unwrap()],
        b"",
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert_eq!(fs::read(blocker).unwrap(), b"keep-blocker");
}

#[test]
fn list_and_find_emit_only_canonical_ids_in_pipes() {
    let fixture = Fixture::new();
    fixture.prompt("notes/first", "One");
    fixture.prompt("notes/second", "Two");
    for args in [&["l"][..], &["f", "notes"][..]] {
        let output = fixture.success(args, b"");
        assert!(output.stderr.is_empty());
        let mut names: Vec<_> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        names.sort();
        assert_eq!(names, ["notes/first", "notes/second"]);
    }
}

#[test]
fn note_frontmatter_and_legacy_cloud_state_survive_reads_and_renaming() {
    let fixture = Fixture::new();
    let raw = "---\nid: notes/original\ndescription: Personal fixture\nauthor: Example\ncustom: {keep: true}\n---\n\nA note, not a model instruction.\n";
    fixture.raw("notes/original", raw);
    let credentials = b"legacy-auth-record = 'offline-private-sentinel'\n";
    fs::write(fixture.store.join("config.toml"), credentials).unwrap();
    fixture.raw("@team/meeting", "Meeting notes\n");
    assert_eq!(
        fixture.success(&["s", "notes/original"], b"").stdout,
        b"A note, not a model instruction.\n"
    );
    assert_eq!(
        fixture.success(&["s", "@team/meeting"], b"").stdout,
        b"Meeting notes\n"
    );
    assert_eq!(
        fs::read(fixture.path("notes/original")).unwrap(),
        raw.as_bytes()
    );
    fixture.success(&["r", "notes/original", "notes/renamed"], b"");
    assert!(!fixture.path("notes/original").exists());
    let renamed = fs::read_to_string(fixture.path("notes/renamed")).unwrap();
    assert!(renamed.contains("keep: true"));
    assert_eq!(
        fixture.success(&["s", "notes/renamed"], b"").stdout,
        b"A note, not a model instruction.\n"
    );
    assert_eq!(
        fs::read(fixture.store.join("config.toml")).unwrap(),
        credentials
    );
}

#[test]
fn version_and_rollback_round_trip_body_once_for_plain_and_banked_prompts() {
    let fixture = Fixture::new();
    for id in ["plain", "notes/banked"] {
        let original = format!(
            "---\nid: {id}\ndescription: Original\ncustom: preserved\n---\n\nBefore {{input}}\n"
        );
        fixture.raw(id, &original);
        fixture.success(&["version", id, "v1", "-m", "Original draft"], b"");
        fixture.prompt(id, "After {input}");
        fixture.success(&["rollback", id, "v1"], b"");
        assert_eq!(fixture.success(&["s", id], b"").stdout, b"Before {input}\n");
        let restored = fs::read_to_string(fixture.path(id)).unwrap();
        assert!(restored.contains("custom: preserved"));
        assert_eq!(
            restored.matches("id:").count(),
            1,
            "nested frontmatter: {restored}"
        );
        fixture.success(&["d", id], b"");
        let history = fixture.success(&["versions", id, "-v"], b"");
        assert!(String::from_utf8_lossy(&history.stdout).contains("v1"));
        fixture.success(&["rollback", id, "v1"], b"");
        assert_eq!(fixture.success(&["s", id], b"").stdout, b"Before {input}\n");
    }
}

#[test]
fn snapshot_failure_aborts_an_overwrite() {
    let fixture = Fixture::new();
    fixture.prompt("source", "Append this");
    fixture.prompt("target", "Keep target");
    let original = fs::read(fixture.path("target")).unwrap();
    fs::write(fixture.store.join(".versions"), "Block snapshot directory").unwrap();
    let output = fixture.run(&["s", "source", "-a", "target"], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(fixture.path("target")).unwrap(), original);
}

#[test]
fn portable_export_import_preserves_notes_and_excludes_private_state() {
    let source = Fixture::new();
    for (id, body) in [
        ("plain", "Plain note\n"),
        ("notes/meeting", "Meeting\n\n"),
        ("@team/shared", "Shared notes\n"),
    ] {
        source.prompt(id, body);
    }
    fs::write(
        source.store.join("config.toml"),
        "legacy-auth-record='export-must-exclude-this'\n",
    )
    .unwrap();
    fs::write(
        source.store.join("history.json"),
        "private-history-sentinel",
    )
    .unwrap();
    let destination = source.temporary.path().join("portable");
    source.success(&["export", destination.to_str().unwrap()], b"");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join("prompthive-export.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["format"], "prompthive");
    assert_eq!(manifest["version"], 1);
    let exported = files_under(&destination);
    assert!(!exported.contains_key(Path::new("config.toml")));
    assert!(!exported.contains_key(Path::new("history.json")));
    assert!(!exported
        .values()
        .any(|bytes| String::from_utf8_lossy(bytes).contains("export-must-exclude-this")));
    let target = Fixture::new();
    let before = files_under(&target.store);
    target.success(&["import", destination.to_str().unwrap(), "--dry-run"], b"");
    assert_eq!(files_under(&target.store), before);
    target.success(&["import", destination.to_str().unwrap()], b"");
    for id in ["plain", "notes/meeting", "@team/shared"] {
        assert_eq!(
            fs::read(source.path(id)).unwrap(),
            fs::read(target.path(id)).unwrap(),
            "raw bytes changed for {id}"
        );
    }
    let output = source.run(&["export", destination.to_str().unwrap()], b"");
    assert!(
        !output.status.success(),
        "export overwrote an existing directory"
    );
    assert_eq!(files_under(&destination), exported);
}

#[test]
fn portable_conflicts_cannot_activate_bindings_against_different_content() {
    let source = Fixture::new();
    source.prompt("work/review", "Exported A");
    fs::write(
        source.store.join("compatibility.toml"),
        "version=1\nverified=true\n[bindings]\ne=\"work/review\"\n",
    )
    .unwrap();
    let exported = source.temporary.path().join("portable");
    source.success(&["export", exported.to_str().unwrap()], b"");
    let target = Fixture::new();
    target.prompt("work/review", "Existing B");
    target.success(&["import", exported.to_str().unwrap()], b"");
    assert_eq!(
        target.success(&["s", "work/review"], b"").stdout,
        b"Existing B"
    );
    target.fail(&["u", "e"], b"", 2);
    target.success(&["import", exported.to_str().unwrap(), "--force"], b"");
    assert_eq!(target.success(&["u", "e"], b"").stdout, b"Exported A");
    assert!(files_under(&target.store)
        .values()
        .any(|bytes| String::from_utf8_lossy(bytes).ends_with("Existing B")));
    let versioned = Fixture::new();
    versioned.prompt("work/review", "Existing B");
    versioned.success(&["import", exported.to_str().unwrap(), "--version"], b"");
    assert_eq!(
        versioned.success(&["s", "work/review"], b"").stdout,
        b"Existing B"
    );
    versioned.fail(&["u", "e"], b"", 2);
    let names = versioned.success(&["l"], b"");
    let names = String::from_utf8(names.stdout).unwrap();
    let new_id = names
        .lines()
        .find(|id| *id != "work/review")
        .expect("versioned copy missing");
    assert_eq!(versioned.success(&["s", new_id], b"").stdout, b"Exported A");
}

#[test]
fn tampered_or_incomplete_portable_exports_never_partially_import() {
    let source = Fixture::new();
    source.prompt("a", "First source");
    source.prompt("z", "Last source");
    let exported = source.temporary.path().join("portable");
    source.success(&["export", exported.to_str().unwrap()], b"");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(exported.join("prompthive-export.json")).unwrap())
            .unwrap();
    let relative = manifest["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "z")
        .unwrap()["path"]
        .as_str()
        .unwrap();
    let original = fs::read(exported.join(relative)).unwrap();
    fs::write(exported.join(relative), b"Tampered source").unwrap();
    let target = Fixture::new();
    target.prompt("untouched", "Existing target");
    let before = files_under(&target.store);
    let output = target.run(&["import", exported.to_str().unwrap(), "--force"], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        files_under(&target.store),
        before,
        "checksum failure partially imported data"
    );
    fs::write(exported.join(relative), original).unwrap();
    fs::write(exported.join("INCOMPLETE"), b"Interrupted export").unwrap();
    let output = target.run(&["import", exported.to_str().unwrap()], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(files_under(&target.store), before);
}

#[test]
fn invalid_ids_and_version_tags_never_escape_the_store() {
    let fixture = Fixture::new();
    fixture.prompt("safe", "Keep safe");
    for name in [
        "../escaped",
        "../../escaped",
        "/absolute",
        "C:/escaped",
        "bank/../escaped",
        "bad//name",
        ".",
        "bank/.",
        "bank/",
    ] {
        let output = fixture.run(&["n", name, "Unsafe sentinel text"], b"");
        assert!(!output.status.success(), "accepted invalid ID {name}");
        assert!(output.stdout.is_empty());
    }
    fixture.fail(&["r", "safe", "../../renamed"], b"", 2);
    fixture.fail(&["version", "safe", "../bad"], b"", 2);
    fixture.fail(&["rollback", "safe", "../bad"], b"", 2);
    assert!(!fixture.temporary.path().join("escaped.md").exists());
    assert!(!fixture.temporary.path().join("renamed.md").exists());
    assert_eq!(fixture.success(&["s", "safe"], b"").stdout, b"Keep safe");
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected_for_read_create_delete_and_rename() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.prompt("safe", "Keep safe");
    let outside = fixture.temporary.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel.md"), "Outside sentinel").unwrap();
    fs::create_dir_all(fixture.store.join("banks")).unwrap();
    symlink(&outside, fixture.store.join("banks/unsafe")).unwrap();
    for args in [
        vec!["s", "unsafe/sentinel"],
        vec!["n", "unsafe/new", "Do not escape"],
        vec!["d", "unsafe/sentinel"],
        vec!["r", "safe", "unsafe/renamed"],
    ] {
        let output = fixture.run(&args, b"");
        assert!(!output.status.success(), "symlink allowed: {args:?}");
        assert!(output.stdout.is_empty());
    }
    assert_eq!(
        fs::read(outside.join("sentinel.md")).unwrap(),
        b"Outside sentinel"
    );
    assert!(!outside.join("new.md").exists());
    assert!(!outside.join("renamed.md").exists());
    assert!(fixture.path("safe").exists());
}

#[test]
fn malformed_prompt_is_an_error_without_payload_or_backtrace() {
    let fixture = Fixture::new();
    fixture.raw(
        "broken",
        "---\nid: broken\nvars: [unterminated\n---\n\nNever emit this",
    );
    let output = fixture.run(&["s", "broken"], b"");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!error.is_empty());
    assert!(!error.contains("Never emit this"));
    assert!(!error.contains("panicked"));
}

#[test]
fn diff_flags_and_merge_safety_keep_their_existing_meaning() {
    let fixture = Fixture::new();
    fixture.prompt("source", "New content");
    fixture.raw(
        "target",
        "---\nid: target\ndescription: Target\ncustom: preserve-me\n---\n\nOld content",
    );
    let before = fs::read(fixture.path("target")).unwrap();
    let diff = fixture.success(&["diff", "source", "target", "-f", "brief", "-c", "1"], b"");
    let text = String::from_utf8_lossy(&diff.stdout);
    assert!(text.contains("source") && text.contains("target"));
    assert!(!text.contains("ms)"));
    assert!(!fixture.temporary.path().join("brief").exists());
    fixture.success(&["merge", "source", "target", "--preview"], b"");
    assert_eq!(fs::read(fixture.path("target")).unwrap(), before);
    let unsupported = fixture.run(&["merge", "source", "target", "-i"], b"");
    assert!(!unsupported.status.success());
    assert!(unsupported.stdout.is_empty());
    assert_eq!(fs::read(fixture.path("target")).unwrap(), before);
    fixture.fail(&["merge", "sou", "target"], b"", 2);
    fixture.success(&["merge", "source", "target"], b"");
    let raw = fs::read_to_string(fixture.path("target")).unwrap();
    assert!(raw.contains("id: target"));
    assert!(raw.contains("custom: preserve-me"));
    assert_eq!(
        fixture.success(&["s", "target"], b"").stdout,
        b"New content"
    );
    assert!(files_under(&fixture.store)
        .values()
        .any(|bytes| bytes == &before));
}

#[test]
fn removed_cloud_commands_fail_locally_without_changing_legacy_state() {
    let fixture = Fixture::new();
    let config = "[legacy]\nauth_record='keep-private-state'\n";
    fs::write(fixture.store.join("config.toml"), config).unwrap();
    for command in [
        "login",
        "logout",
        "sync",
        "teams",
        "users",
        "subscription",
        "share",
        "suggestions",
        "improve",
        "publish",
        "unpublish",
        "install",
        "search",
        "browse",
        "web",
        "health",
        "security",
        "history",
        "batch",
        "vars",
        "values",
        "v",
    ] {
        let output = fixture.run(&[command], b"");
        assert!(
            !output.status.success(),
            "removed command claimed success: {command}"
        );
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert_eq!(
            fs::read_to_string(fixture.store.join("config.toml")).unwrap(),
            config
        );
    }
    fixture.prompt("sync", "A real local prompt");
    assert_eq!(
        fixture.success(&["u", "sync"], b"").stdout,
        b"A real local prompt"
    );
}

#[test]
fn shell_completions_include_actions_and_exclude_removed_commands() {
    let fixture = Fixture::new();
    for shell in ["bash", "zsh", "fish"] {
        let output = fixture.success(&["completion", shell], b"");
        let script = String::from_utf8(output.stdout).unwrap();
        let tokens: Vec<_> = script
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .collect();
        for action in [
            "use", "u", "find", "f", "new", "n", "edit", "e", "compose", "c",
        ] {
            assert!(tokens.contains(&action), "{shell} omits {action}");
        }
        for removed in ["login", "logout", "sync", "publish", "registry", "teams"] {
            assert!(!tokens.contains(&removed), "{shell} advertises {removed}");
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_completion_reads_later_prompts_only_in_existing_selector_positions() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fs::remove_dir(&fixture.store).unwrap();
    let generated = fixture.success(&["completion", "bash"], b"");
    let script = fixture.temporary.path().join("completion.bash");
    fs::write(&script, generated.stdout).unwrap();
    let bin = fixture.temporary.path().join("bin");
    fs::create_dir(&bin).unwrap();
    symlink(env!("CARGO_BIN_EXE_ph"), bin.join("ph")).unwrap();
    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited_path)))
            .unwrap();
    let probe = r#"source "$1"
shift
COMP_WORDS=("$@")
COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
COMP_LINE="${COMP_WORDS[*]}"
COMP_POINT=${#COMP_LINE}
_ph ph "${COMP_WORDS[COMP_CWORD]}"
printf '%s\n' "${COMPREPLY[@]}"
"#;
    let complete = |words: &[&str]| {
        Command::new("bash")
            .args(["--noprofile", "--norc", "-c", probe, "completion-contract"])
            .arg(&script)
            .args(words)
            .current_dir(fixture.temporary.path())
            .env("PROMPTHIVE_BASE_DIR", &fixture.store)
            .env("XDG_CONFIG_HOME", fixture.temporary.path().join("config"))
            .env("PATH", &path)
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let absent = complete(&["ph", "use", "rev"]);
    assert!(absent.status.success());
    assert!(String::from_utf8_lossy(&absent.stdout).trim().is_empty());
    assert!(
        !fixture.store.exists(),
        "completion created a prompt library"
    );
    // Create this after generation: a static snapshot of names cannot pass.
    fixture.prompt("review", "Added after completion generation");
    fixture.prompt(
        "-review",
        "A canonical name requiring the end-of-options marker",
    );
    for (words, expected) in [
        (vec!["ph", "use", "rev"], vec!["review"]),
        (vec!["ph", "u", "-q", "rev"], vec!["review"]),
        (vec!["ph", "use", "-v", "lang=Rust", "rev"], vec!["review"]),
        (vec!["ph", "c", "base,rev"], vec!["base,review"]),
        (vec!["ph", "diff", "other", "rev"], vec!["review"]),
        (vec!["ph", "s", "source", "-a", "rev"], vec!["review"]),
        (vec!["ph", "l", "-a", "rev"], vec!["review"]),
        (vec!["ph", "f", "rev", "-a", "rev"], vec!["review"]),
        (vec!["ph", "r", "other", "rev"], vec![]),
        (vec!["ph", "use", "review", "rev"], vec![]),
        (vec!["ph", "n", "rev"], vec![]),
        (vec!["ph", "use", "review", "-v", "rev"], vec![]),
        (vec!["ph", "use", "-r"], vec![]),
        (vec!["ph", "use", "--", "-r"], vec!["-review"]),
        (vec!["ph", "show", "--", "-r"], vec!["-review"]),
        (vec!["ph", "edit", "--", "-r"], vec!["-review"]),
        (vec!["ph", "delete", "--", "-r"], vec!["-review"]),
        (vec!["ph", "c", "--", "-r"], vec!["-review"]),
        (vec!["ph", "c", "--", "-review,rev"], vec!["-review,review"]),
        (vec!["ph", "use", "--", "-review", "rev"], vec![]),
        (vec!["ph", "r", "--", "-review", "rev"], vec![]),
        (vec!["ph", "diff", "--", "-review", "review", "rev"], vec![]),
    ] {
        let output = complete(&words);
        assert!(
            output.status.success(),
            "{words:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{words:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = String::from_utf8(output.stdout).unwrap();
        let actual: Vec<_> = output.lines().filter(|line| !line.is_empty()).collect();
        assert_eq!(actual, expected, "completion for {words:?}");
    }
}

#[cfg(unix)]
fn editor_script(fixture: &Fixture, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = fixture.temporary.path().join("editor.sh");
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[cfg(windows)]
#[test]
fn windows_editor_command_preserves_native_paths_for_saved_and_rendered_edits() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Unchanged editor fixture");
    let original = fs::read(fixture.path("review")).unwrap();
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
    let executable = system.join("System32").join("cmd.exe");
    assert!(executable.is_file());
    for editor in [
        format!("{} /D /C exit 0", executable.display()),
        format!("\"{}\" /D /C exit 0", executable.display()),
    ] {
        for args in [vec!["e", "review"], vec!["u", "review", "--edit", "-q"]] {
            let result = fixture.run_env(&args, b"", &[("EDITOR", &editor)]);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(fs::read(fixture.path("review")).unwrap(), original);
        }
    }
}

#[cfg(unix)]
#[test]
fn editor_success_is_visible_on_next_invocation_and_failure_is_not_committed() {
    let fixture = Fixture::new();
    fixture.raw(
        "note",
        "---\nid: note\ndescription: Note\ncustom: keep\n---\n\nBefore",
    );
    let editor = editor_script(&fixture, "sed 's/Before/After/g' \"$1\" > \"$1.changed\"\ncat \"$1.changed\" > \"$1\"\nrm \"$1.changed\"");
    let output = fixture.run_env(&["e", "note"], b"", &[("EDITOR", editor.to_str().unwrap())]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.success(&["u", "note"], b"").stdout, b"After");
    assert!(fs::read_to_string(fixture.path("note"))
        .unwrap()
        .contains("custom: keep"));
    let before = fs::read(fixture.path("note")).unwrap();
    let editor = editor_script(&fixture, "printf '%s' partial > \"$1\"\nexit 23");
    let output = fixture.run_env(&["e", "note"], b"", &[("EDITOR", editor.to_str().unwrap())]);
    assert!(!output.status.success());
    assert_eq!(fs::read(fixture.path("note")).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn concurrent_editor_change_returns_conflict_and_preserves_external_content() {
    let fixture = Fixture::new();
    fixture.prompt("note", "Before");
    let original = fixture.path("note");
    let quoted = original.to_str().unwrap().replace('\'', "'\\''");
    let editor = editor_script(
        &fixture,
        &format!(
            "printf '%s' 'External change' > '{quoted}'\nprintf '%s' 'Candidate change' > \"$1\""
        ),
    );
    let output = fixture.run_env(&["e", "note"], b"", &[("EDITOR", editor.to_str().unwrap())]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(original).unwrap(), b"External change");
}

#[cfg(unix)]
#[test]
fn downstream_closed_pipe_is_quiet() {
    use std::os::unix::process::ExitStatusExt;
    let fixture = Fixture::new();
    fixture.prompt("payload", &"x".repeat(128 * 1024));
    let mut command = fixture.command();
    command.args(["s", "payload"]);
    let mut child = command.spawn().unwrap();
    drop(child.stdin.take());
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success()
            || output.status.signal() == Some(13)
            || output.status.code() == Some(141)
    );
}

#[test]
fn listing_and_discovery_keep_their_explicit_destinations() {
    let fixture = Fixture::new();
    fixture.prompt("review", "Review {input}");
    let output = fixture.temporary.path().join("names.txt");
    assert!(fixture
        .success(&["l", "-f", output.to_str().unwrap(), "-q"], b"")
        .stdout
        .is_empty());
    assert_eq!(fs::read(&output).unwrap(), b"review\n");
    fixture.success(&["f", "rev", "-s", "notes/names", "-q"], b"");
    assert_eq!(
        fixture.success(&["s", "notes/names", "-q"], b"").stdout,
        b"review\n"
    );
    fixture.fail(
        &[
            "f",
            "rev",
            "-s",
            "notes/never",
            "-f",
            output.to_str().unwrap(),
        ],
        b"",
        2,
    );
    assert!(!fixture.path("notes/never").exists());
    #[cfg(target_os = "linux")]
    {
        fixture.fail(&["l", "-c"], b"", 4);
        fixture.fail(&["f", "rev", "-c", "-q"], b"", 4);
    }
}

#[test]
fn tagged_unknown_metadata_stays_inert_through_read_rename_and_export() {
    let fixture = Fixture::new();
    let private = fixture.temporary.path().join("not-a-template");
    fs::write(&private, b"SYNTHETIC_NEVER_INCLUDE_38b1").unwrap();
    let raw = format!("---\nid: note\ncustom: !include {}\nnested: {{value: !arbitrary [one, two]}}\n---\n\nBody {{input}}", private.display());
    fixture.raw("note", &raw);
    assert_eq!(
        fixture.success(&["s", "note", "-q"], b"").stdout,
        b"Body {input}"
    );
    assert_eq!(
        fixture.success(&["u", "note", "literal", "-q"], b"").stdout,
        b"Body literal"
    );
    assert_eq!(fs::read(fixture.path("note")).unwrap(), raw.as_bytes());
    fixture.success(&["r", "note", "notes/renamed"], b"");
    fixture.prompt("addition", "Added");
    fixture.success(&["s", "addition", "-a", "notes/renamed", "-q"], b"");
    assert_eq!(
        fixture.success(&["s", "notes/renamed", "-q"], b"").stdout,
        b"Body {input}\n\nAdded"
    );
    let renamed = fs::read_to_string(fixture.path("notes/renamed")).unwrap();
    assert!(renamed.contains("!include"));
    assert!(renamed.contains("!arbitrary"));
    let export = fixture.temporary.path().join("portable-tags");
    fixture.success(&["export", export.to_str().unwrap()], b"");
    assert_eq!(
        fs::read(export.join("banks/notes/renamed.md")).unwrap(),
        renamed.as_bytes()
    );
    assert_eq!(fs::read(private).unwrap(), b"SYNTHETIC_NEVER_INCLUDE_38b1");
}
