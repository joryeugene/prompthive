//! Exercise budget errors through the real CLI, preserving existing destinations.
use prompthive::limits::{FRONTMATTER_BYTES, IMPORT_BYTES, TEXT_BYTES};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

struct Fixture {
    temporary: tempfile::TempDir,
    store: PathBuf,
}

#[cfg(target_os = "linux")]
#[test]
fn search_does_not_retain_unused_legacy_metadata_under_a_memory_cap() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture::new();
    let fields = ["version", "created_at", "updated_at", "git_hash"];
    let value = "x".repeat(850_000);
    let mut expected = String::new();
    for number in 0..170 {
        let name = format!("note{number:03}");
        let raw = format!(
            "---\n{}: {value}\n---\n\nsmall",
            fields[number % fields.len()]
        );
        assert!(raw.len() < FRONTMATTER_BYTES);
        fs::write(
            fixture.store.join("prompts").join(format!("{name}.md")),
            raw,
        )
        .unwrap();
        expected.push_str(&format!("{name}\n"));
    }
    expected.push_str("tiny\n");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ph"));
    command
        .args(["f", "", "-q"])
        .env("PROMPTHIVE_BASE_DIR", &fixture.store)
        .env("XDG_CONFIG_HOME", fixture.temporary.path().join("config"))
        .stdin(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            let cap = libc::rlimit {
                rlim_cur: 256 * 1024 * 1024,
                rlim_max: 256 * 1024 * 1024,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &cap) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let no_core = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_CORE, &no_core) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(result.stdout, expected.as_bytes());
    assert_eq!(
        fs::read_to_string(fixture.store.join("prompts/note000.md")).unwrap(),
        format!("---\nversion: {value}\n---\n\nsmall")
    );
    assert!(!fixture.store.join(".versions").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn expanded_yaml_aliases_fail_cleanly_under_a_process_memory_cap() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture::new();
    let path = fixture.store.join("prompts/aliases.md");
    let output = fixture.temporary.path().join("output.txt");
    for (aliases, expected) in [(14, 0), (15, 2), (1000, 2)] {
        let content = format!(
            "---\ncustom: &large {}\nreferences: [{}]\n---\n\nBODY",
            "x".repeat(512 * 1024),
            vec!["*large"; aliases].join(", ")
        );
        assert!(content.len() < FRONTMATTER_BYTES);
        fs::write(&path, &content).unwrap();
        fs::write(&output, "ORIGINAL").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ph"));
        command
            .args(["s", "aliases", "-f"])
            .arg(&output)
            .arg("--force")
            .env("PROMPTHIVE_BASE_DIR", &fixture.store)
            .env("XDG_RUNTIME_DIR", fixture.temporary.path().join("runtime"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .stdin(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                for (resource, maximum) in [
                    (libc::RLIMIT_AS, 256 * 1024 * 1024),
                    (libc::RLIMIT_CPU, 2),
                    (libc::RLIMIT_CORE, 0),
                ] {
                    let limit = libc::rlimit {
                        rlim_cur: maximum,
                        rlim_max: maximum,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(expected),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        if expected == 0 {
            assert_eq!(fs::read(&output).unwrap(), b"BODY");
        } else {
            assert!(String::from_utf8_lossy(&result.stderr).contains("expanded YAML data"));
            assert_eq!(fs::read(&output).unwrap(), b"ORIGINAL");
        }
    }
}
impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let store = temporary.path().join("store");
        fs::create_dir_all(store.join("prompts")).unwrap();
        fs::write(store.join("prompts/tiny.md"), "[{input}]").unwrap();
        Self { temporary, store }
    }
    fn run(&self, args: &[&str], input: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ph"))
            .args(args)
            .current_dir(self.temporary.path())
            .env("PROMPTHIVE_BASE_DIR", &self.store)
            .env("XDG_CONFIG_HOME", self.temporary.path().join("config"))
            .env("XDG_RUNTIME_DIR", self.temporary.path().join("runtime"))
            .env("TERM", "dumb")
            .env("NO_COLOR", "1")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
    fn rejected(&self, args: &[&str], input: &[u8]) -> Output {
        let result = self.run(args, input);
        assert_eq!(
            result.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("safety limit"));
        result
    }
}

#[test]
fn oversize_stdin_does_not_replace_an_explicit_file_or_make_backups() {
    let f = Fixture::new();
    fs::write(f.temporary.path().join("output.txt"), "original file").unwrap();
    f.rejected(
        &["u", "tiny", "-f", "output.txt", "--force"],
        &vec![b'x'; TEXT_BYTES + 1],
    );
    assert_eq!(
        fs::read(f.temporary.path().join("output.txt")).unwrap(),
        b"original file"
    );
    assert!(!f.temporary.path().join(".versions").exists());
    assert_eq!(
        fs::read(f.store.join("prompts/tiny.md")).unwrap(),
        b"[{input}]"
    );
}

#[test]
fn large_stored_and_explicit_sources_fail_without_creating_destinations() {
    let f = Fixture::new();
    let stored = f.store.join("prompts/large.md");
    let file = fs::File::create(&stored).unwrap();
    file.set_len((TEXT_BYTES + 1) as u64).unwrap();
    f.rejected(&["s", "large", "-f", "result.txt"], b"");
    f.rejected(&["c", stored.to_str().unwrap(), "-f", "result.txt"], b"");
    assert!(!f.temporary.path().join("result.txt").exists());
    assert_eq!(
        fs::metadata(&stored).unwrap().len(),
        (TEXT_BYTES + 1) as u64
    );
}

#[test]
fn frontmatter_boundary_is_in_bytes_and_unknown_values_remain_data() {
    let f = Fixture::new();
    let prefix = "custom: ";
    let yaml = format!(
        "{prefix}{}\n",
        "é".repeat((FRONTMATTER_BYTES - prefix.len() - 1) / 2)
    );
    let padding = FRONTMATTER_BYTES - yaml.len();
    let yaml = format!("{yaml}{}", "\n".repeat(padding));
    assert_eq!(yaml.len(), FRONTMATTER_BYTES);
    fs::write(
        f.store.join("prompts/exact.md"),
        format!("---\n{yaml}---\n\nbody"),
    )
    .unwrap();
    let result = f.run(&["s", "exact", "-q"], b"");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout, b"body");
    let over = format!("---\n{yaml}\n---\n\nbody");
    fs::write(f.store.join("prompts/over.md"), &over).unwrap();
    f.rejected(&["s", "over"], b"");
    assert_eq!(
        fs::read_to_string(f.store.join("prompts/over.md")).unwrap(),
        over
    );
}

#[test]
fn rendered_output_budget_accepts_exact_size_and_rejects_expansion_before_effects() {
    let f = Fixture::new();
    fs::write(f.store.join("prompts/twice.md"), "{input}{input}").unwrap();
    let exact = vec![b'x'; TEXT_BYTES / 2];
    let result = f.run(&["u", "twice", "-f", "exact.txt"], &exact);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::metadata(f.temporary.path().join("exact.txt"))
            .unwrap()
            .len(),
        TEXT_BYTES as u64
    );
    fs::write(f.temporary.path().join("original.txt"), "retain").unwrap();
    f.rejected(
        &["u", "twice", "-f", "original.txt", "--force"],
        &vec![b'x'; TEXT_BYTES / 2 + 1],
    );
    assert_eq!(
        fs::read(f.temporary.path().join("original.txt")).unwrap(),
        b"retain"
    );
}

#[test]
fn compilation_bounds_inserted_values_and_segment_count() {
    let f = Fixture::new();
    let value = "x".repeat(FRONTMATTER_BYTES / 2);
    fs::write(
        f.store.join("prompts/expand.md"),
        format!(
            "---\nvars: {{value: {value}}}\n---\n\n{}",
            "{value}".repeat(65)
        ),
    )
    .unwrap();
    f.rejected(&["u", "expand"], b"");
    fs::write(
        f.store.join("prompts/segments.md"),
        "{input}".repeat(65_537),
    )
    .unwrap();
    f.rejected(&["u", "segments"], b"");
    fs::write(
        f.store.join("prompts/segments.md"),
        "{input}".repeat(65_536),
    )
    .unwrap();
    assert!(f.run(&["u", "segments"], b"").status.success());
}

#[test]
fn late_import_batch_overflow_keeps_all_preexisting_notes_and_no_partial_import() {
    let f = Fixture::new();
    let source = f.temporary.path().join("import");
    fs::create_dir(&source).unwrap();
    fs::write(f.store.join("prompts/a.md"), "original note").unwrap();
    let each = IMPORT_BYTES / 5 + 1;
    for name in ["a", "b", "c", "d", "e"] {
        let file = fs::File::create(source.join(format!("{name}.md"))).unwrap();
        file.set_len(each as u64).unwrap();
    }
    f.rejected(&["import", source.to_str().unwrap(), "--force"], b"");
    assert_eq!(
        fs::read(f.store.join("prompts/a.md")).unwrap(),
        b"original note"
    );
    for name in ["b", "c", "d", "e"] {
        assert!(!f.store.join("prompts").join(format!("{name}.md")).exists());
    }
    assert!(!f.store.join(".versions").exists());
}
