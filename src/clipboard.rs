//! Clipboard output is lazy and stays pasteable after the CLI process exits.

use anyhow::{bail, Context, Result};
use std::io::{self, Write};

#[cfg(any(target_os = "macos", windows))]
use copypasta::{ClipboardContext, ClipboardProvider};

#[derive(Default)]
pub struct Clipboard {
    #[cfg(any(target_os = "macos", windows))]
    context: Option<ClipboardContext>,
    #[cfg(any(target_os = "macos", windows))]
    initialized: bool,
}

impl Clipboard {
    /// Constructing a clipboard must not connect to a display in a pipeline.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn copy_or_pipe(&mut self, content: &str, is_tty: bool) -> Result<()> {
        if is_tty {
            if !self.copy_to_clipboard(content)? {
                bail!("Clipboard unavailable");
            }
        } else {
            self.write_to_stdout(content)?;
        }
        Ok(())
    }

    pub fn copy_to_clipboard(&mut self, content: &str) -> Result<bool> {
        #[cfg(any(target_os = "macos", windows))]
        {
            if !self.initialized {
                self.context = ClipboardContext::new().ok();
                self.initialized = true;
            }
            Ok(self
                .context
                .as_mut()
                .is_some_and(|context| context.set_contents(content.to_owned()).is_ok()))
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // These platform helpers retain selection ownership after ph exits.
            // An in-process X11/Wayland provider cannot promise that lifetime.
            let timeout = std::time::Duration::from_secs(2);
            if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
                && copy_with_command(
                    "wl-copy",
                    &["--type", "text/plain;charset=utf-8"],
                    content,
                    timeout,
                )
            {
                return Ok(true);
            }
            if std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty()) {
                for (program, args) in [
                    (
                        "xclip",
                        &["-selection", "clipboard", "-in", "-target", "UTF8_STRING"][..],
                    ),
                    ("xsel", &["--clipboard", "--input"][..]),
                ] {
                    if copy_with_command(program, args, content, timeout) {
                        return Ok(true);
                    }
                }
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        Ok(false)
    }

    pub fn write_to_stdout(&self, content: &str) -> Result<()> {
        let mut output = io::stdout().lock();
        output
            .write_all(content.as_bytes())
            .context("Failed to write to stdout")?;
        output.flush().context("Failed to flush stdout")
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn copy_with_command(
    program: &str,
    args: &[&str],
    content: &str,
    timeout: std::time::Duration,
) -> bool {
    use std::{
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let Some(mut input) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    };
    let (sent, received) = mpsc::sync_channel(1);
    let bytes = content.as_bytes().to_vec();
    // A stalled helper must not block the caller while writing a large prompt.
    let writer = thread::spawn(move || {
        let result = input.write_all(&bytes).is_ok();
        drop(input);
        let _ = sent.send(result);
    });
    let deadline = Instant::now() + timeout;
    let mut complete_write = None;
    let success = loop {
        if complete_write.is_none() {
            if let Ok(result) = received.try_recv() {
                complete_write = Some(result);
            }
        }
        if complete_write == Some(false) || Instant::now() >= deadline {
            break false;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    break false;
                }
                if let Some(result) = complete_write {
                    break result;
                }
            }
            Ok(None) => {}
            Err(_) => break false,
        }
        thread::sleep(Duration::from_millis(5));
    };
    if !success {
        let _ = child.kill();
    }
    let _ = child.wait();
    if writer.is_finished() {
        let _ = writer.join();
    }
    success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_does_not_initialize_a_native_clipboard() {
        let clipboard = Clipboard::new();
        #[cfg(any(target_os = "macos", windows))]
        {
            assert!(!clipboard.initialized);
            assert!(clipboard.context.is_none());
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = clipboard;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn helper_receives_exact_literal_bytes_through_stdin() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("copied.txt");
        let content = "  literal $(no-command) {env:SECRET}\nRésumé 😀\n\n";
        assert!(copy_with_command(
            "/bin/sh",
            &[
                "-c",
                "cat > \"$1\"",
                "clipboard-test",
                destination.to_str().unwrap()
            ],
            content,
            std::time::Duration::from_secs(2),
        ));
        assert_eq!(std::fs::read(destination).unwrap(), content.as_bytes());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn missing_or_failed_helpers_do_not_report_copy_success() {
        assert!(!copy_with_command(
            "/nonexistent/prompthive-clipboard-helper",
            &[],
            "test",
            std::time::Duration::from_millis(100),
        ));
        assert!(!copy_with_command(
            "/bin/sh",
            &["-c", "cat >/dev/null; exit 7"],
            "test",
            std::time::Duration::from_secs(1),
        ));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_stalled_helper_and_large_input_have_a_bounded_failure() {
        let start = std::time::Instant::now();
        assert!(!copy_with_command(
            "/bin/sh",
            &["-c", "exec sleep 5"],
            &"x".repeat(1024 * 1024),
            std::time::Duration::from_millis(60),
        ));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
}
