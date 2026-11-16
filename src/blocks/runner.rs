//! Runs a block's script, blocking; call it off the main thread.
//!
//! The script runs through the user's login shell (`$SHELL -l -c`), because an app launched from
//! the Dock gets a bare `PATH` without Homebrew. Contract: the input JSON on stdin and as
//! `BLOCK_<PARAM>` env vars; stdout is the Markdown; a non-zero exit is a failure (stderr shown).
//! It runs in its own process group so a timeout kills whatever the script started too.

use std::{
    io::{self, Read, Write},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde_json::{Map, Value};

use super::model::Output;

/// Stdout beyond this is dropped (and the output marked truncated).
const STDOUT_CAP: usize = 1024 * 1024;
const STDERR_CAP: usize = 16 * 1024;
const KEYCHAIN: &str = "keychain:";

pub struct Job {
    /// Shell that runs the command as a login shell.
    pub shell: PathBuf,
    /// Working directory: the block's folder.
    pub dir: PathBuf,
    /// `[interpreter, script]`, or `[./script]` when there is no interpreter.
    pub command: Vec<String>,
    pub input: Map<String, Value>,
    /// Extra environment; `keychain:<service>` values are resolved here.
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

impl Job {
    pub fn command(interpreter: &str, script: &str) -> Vec<String> {
        if interpreter.trim().is_empty() {
            vec![format!("./{script}")]
        } else {
            vec![interpreter.trim().to_string(), script.to_string()]
        }
    }
}

/// The user's login shell (`$SHELL`, else zsh, the macOS default).
pub fn login_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| "/bin/zsh".into())
}

pub fn run(job: &Job) -> Output {
    let started = Instant::now();
    let ran_at = Utc::now();
    let finish = |markdown: String, stderr: String, ok: bool| Output {
        markdown,
        stderr,
        ok,
        ran_at,
        duration_ms: started.elapsed().as_millis() as u64,
    };
    match execute(job) {
        Ok((stdout, stderr, ok)) => finish(stdout, stderr, ok),
        Err(err) => finish(String::new(), format!("{err:#}"), false),
    }
}

/// (stdout, stderr, succeeded)
fn execute(job: &Job) -> anyhow::Result<(String, String, bool)> {
    let mut cmd = Command::new(&job.shell);
    // `exec "$@"`: the shell replaces itself with the command, args passed verbatim (no quoting).
    cmd.args(["-l", "-c", "exec \"$@\"", "buffer-block"])
        .args(&job.command)
        .current_dir(&job.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    for (key, value) in &job.input {
        let value = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        cmd.env(format!("BLOCK_{}", env_name(key)), value);
    }
    for (key, value) in &job.env {
        let value = match value.strip_prefix(KEYCHAIN) {
            Some(service) => keychain(service.trim())?,
            None => value.clone(),
        };
        cmd.env(key, value);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("couldn't start {}: {e}", job.shell.display()))?;
    let input = serde_json::to_vec(&job.input)?;
    let mut stdin = child.stdin.take();
    let writer = thread::spawn(move || {
        if let Some(stdin) = stdin.as_mut() {
            // A script that never reads stdin closes it early: not an error.
            let _ = stdin.write_all(&input);
        }
    });
    let out = child
        .stdout
        .take()
        .map(|r| thread::spawn(|| drain(r, STDOUT_CAP)));
    let err = child
        .stderr
        .take()
        .map(|r| thread::spawn(|| drain(r, STDERR_CAP)));

    let deadline = Instant::now() + job.timeout.max(Duration::from_secs(1));
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let _ = writer.join();
    let (stdout, truncated) = out.and_then(|h| h.join().ok()).unwrap_or_default();
    let (stderr, _) = err.and_then(|h| h.join().ok()).unwrap_or_default();

    let mut stdout = String::from_utf8_lossy(&stdout).into_owned();
    if truncated {
        stdout.push_str("\n\n*Output truncated.*");
    }
    let mut stderr = String::from_utf8_lossy(&stderr).trim().to_string();
    let ok = match status {
        Some(status) if status.success() => true,
        Some(status) => {
            if stderr.is_empty() {
                stderr = match status.code() {
                    Some(code) => format!("exited with status {code}"),
                    None => "killed by a signal".into(),
                };
            }
            false
        }
        None => {
            let note = format!("timed out after {}s", job.timeout.as_secs());
            stderr = if stderr.is_empty() {
                note
            } else {
                format!("{stderr}\n{note}")
            };
            false
        }
    };
    Ok((stdout, stderr, ok))
}

/// Reads to the end (so the writer never blocks on a full pipe), keeping the first `cap` bytes.
fn drain(mut r: impl Read, cap: usize) -> (Vec<u8>, bool) {
    let (mut kept, mut truncated, mut buf) = (Vec::new(), false, [0u8; 8192]);
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(&buf[..n.min(room)]);
                truncated |= n > room;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    (kept, truncated)
}

/// SIGKILL to the whole process group (the script and anything it spawned).
fn kill_group(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// `limit` → `LIMIT`, `max-items` → `MAX_ITEMS`.
fn env_name(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// A generic password from the login keychain, by service name.
fn keychain(service: &str) -> anyhow::Result<String> {
    let out = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-w", "-s", service])
        .stdin(Stdio::null())
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "keychain item {service:?} not found (add it with: security add-generic-password -s {service} -a $USER -w)"
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("buffer-runner-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(dir: PathBuf, script: &str, input: Value, timeout: u64) -> Job {
        fs::write(dir.join("run.sh"), script).unwrap();
        Job {
            shell: "/bin/sh".into(),
            dir,
            command: Job::command("sh", "run.sh"),
            input: input.as_object().cloned().unwrap_or_default(),
            env: vec![("GREETING".into(), "hi".into())],
            timeout: Duration::from_secs(timeout),
        }
    }

    #[test]
    fn passes_input_and_env_and_captures_stdout() {
        let dir = scratch("ok");
        let out = run(&job(
            dir.clone(),
            "read -r line; echo \"$line|$BLOCK_MAX_ITEMS|$GREETING|$(pwd -P)\"",
            json!({"max-items": 5}),
            10,
        ));
        assert!(out.ok, "{}", out.stderr);
        let real = fs::canonicalize(&dir).unwrap();
        assert_eq!(
            out.markdown.trim(),
            format!("{{\"max-items\":5}}|5|hi|{}", real.display())
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failure_keeps_stderr() {
        let dir = scratch("fail");
        let out = run(&job(dir.clone(), "echo oops >&2; exit 3", json!({}), 10));
        assert!(!out.ok);
        assert_eq!(out.stderr, "oops");
        let out = run(&job(dir.clone(), "exit 4", json!({}), 10));
        assert_eq!(out.stderr, "exited with status 4");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn timeout_kills_the_group() {
        let dir = scratch("slow");
        let started = Instant::now();
        // The background child holds stdout open; killing only the shell would hang the read.
        let out = run(&job(dir.clone(), "sleep 30 & sleep 30", json!({}), 1));
        assert!(!out.ok);
        assert!(out.stderr.contains("timed out after 1s"), "{}", out.stderr);
        assert!(started.elapsed() < Duration::from_secs(10));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn caps_stdout() {
        let (kept, truncated) = drain(&[7u8; 20_000][..], 10_000);
        assert_eq!(kept.len(), 10_000);
        assert!(truncated);
        assert!(!drain(&b"ab"[..], 10).1);
    }

    #[test]
    fn env_names() {
        assert_eq!(env_name("max-items"), "MAX_ITEMS");
        assert_eq!(env_name("jql"), "JQL");
    }
}
