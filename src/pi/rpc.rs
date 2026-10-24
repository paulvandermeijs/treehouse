//! A Pi process in RPC mode: JSON lines over stdin and stdout.

use anyhow::{Context as _, Result};
use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// How to start Pi for one session.
#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    /// The `pi` executable; resolved on `PATH` when relative.
    pub program: PathBuf,
    pub cwd: PathBuf,
    /// `provider/model`, passed as `--model`.
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub tools: Vec<String>,
    pub extensions: Vec<PathBuf>,
    pub session_dir: Option<PathBuf>,
}

impl LaunchOptions {
    pub fn args(&self) -> Vec<String> {
        let mut args = vec!["--mode".to_string(), "rpc".to_string()];
        if let Some(model) = &self.model {
            args.extend(["--model".to_string(), model.clone()]);
        }
        if let Some(thinking) = &self.thinking {
            args.extend(["--thinking".to_string(), thinking.clone()]);
        }
        if !self.tools.is_empty() {
            args.extend(["--tools".to_string(), self.tools.join(",")]);
        }
        for extension in &self.extensions {
            args.extend(["-e".to_string(), extension.display().to_string()]);
        }
        match &self.session_dir {
            Some(dir) => args.extend(["--session-dir".to_string(), dir.display().to_string()]),
            None => args.push("--no-session".to_string()),
        }
        args
    }
}

/// One line read from Pi's stdout, or the process going away.
#[derive(Debug, Clone)]
pub enum PiMessage {
    Response(Response),
    Event(Value),
    Exited { code: Option<i32>, stderr: String },
}

#[derive(Debug, Clone)]
pub struct Response {
    pub id: Option<String>,
    pub command: String,
    pub success: bool,
    pub data: Value,
    pub error: Option<String>,
}

/// Parses one stdout line.
pub fn parse_line(line: &str) -> Option<PiMessage> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.trim().is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) == Some("response") {
        return Some(PiMessage::Response(Response {
            id: value.get("id").and_then(Value::as_str).map(str::to_owned),
            command: str_field(&value, "command"),
            success: value.get("success").and_then(Value::as_bool).unwrap_or(false),
            data: value.get("data").cloned().unwrap_or(Value::Null),
            error: value.get("error").and_then(Value::as_str).map(str::to_owned),
        }));
    }
    Some(PiMessage::Event(value))
}

pub fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A running Pi process. Dropping it closes stdin, which ends Pi.
pub struct PiClient {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    next_id: AtomicU64,
}

impl PiClient {
    /// Starts Pi and returns the client and the channel its output arrives on.
    pub fn spawn(options: &LaunchOptions) -> Result<(Self, smol::channel::Receiver<PiMessage>)> {
        let program = resolve_program(&options.program)?;
        let mut child = Command::new(&program)
            .args(options.args())
            .current_dir(&options.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to start {}", program.display()))?;

        let stdout = child.stdout.take().context("pi has no stdout")?;
        let mut stderr = child.stderr.take().context("pi has no stderr")?;
        let stdin = child.stdin.take();
        let child = Arc::new(Mutex::new(child));
        let (tx, rx) = smol::channel::unbounded();

        // Drain stderr so Pi never blocks on it; keep the tail for errors.
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        std::thread::spawn({
            let stderr_tail = stderr_tail.clone();
            move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut tail = stderr_tail.lock().unwrap();
                    tail.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if tail.len() > 8192 {
                        let cut = tail.len() - 8192;
                        let cut = (cut..tail.len()).find(|i| tail.is_char_boundary(*i)).unwrap_or(0);
                        tail.drain(..cut);
                    }
                }
            }
        });

        // Read stdout continuously: Pi honours back-pressure. Split on `\n`
        // only, since strings may contain U+2028.
        std::thread::spawn({
            let child = child.clone();
            move || {
                let mut reader = BufReader::new(stdout);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let text = String::from_utf8_lossy(&line);
                            let text = text.strip_suffix('\n').unwrap_or(&text);
                            if let Some(message) = parse_line(text) {
                                if tx.send_blocking(message).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                let code = child.lock().unwrap().wait().ok().and_then(|s| s.code());
                let stderr = stderr_tail.lock().unwrap().clone();
                let _ = tx.send_blocking(PiMessage::Exited { code, stderr });
            }
        });

        Ok((
            Self {
                stdin: Arc::new(Mutex::new(stdin)),
                child,
                next_id: AtomicU64::new(1),
            },
            rx,
        ))
    }

    /// Sends a command and returns the id its response will carry.
    pub fn send(&self, mut command: Value) -> String {
        let id = format!("th-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        if let Some(object) = command.as_object_mut() {
            object.insert("id".into(), Value::String(id.clone()));
        }
        self.write(&command);
        id
    }

    /// Answers an `extension_ui_request` dialog.
    pub fn respond_ui(&self, request_id: &str, value: Option<&str>) {
        let line = match value {
            Some(value) => json!({"type": "extension_ui_response", "id": request_id, "value": value}),
            None => json!({"type": "extension_ui_response", "id": request_id, "cancelled": true}),
        };
        self.write(&line);
    }

    fn write(&self, value: &Value) {
        let mut stdin = self.stdin.lock().unwrap();
        if let Some(pipe) = stdin.as_mut() {
            let mut line = value.to_string();
            line.push('\n');
            if pipe.write_all(line.as_bytes()).and_then(|_| pipe.flush()).is_err() {
                *stdin = None;
            }
        }
    }

    pub fn kill(&self) {
        self.stdin.lock().unwrap().take();
        let _ = self.child.lock().unwrap().kill();
    }
}

impl Drop for PiClient {
    fn drop(&mut self) {
        self.kill();
    }
}

fn resolve_program(program: &Path) -> Result<PathBuf> {
    if program.components().count() > 1 {
        return Ok(program.to_path_buf());
    }
    which::which(program).with_context(|| {
        format!(
            "`{}` was not found on PATH. Install Pi with `npm install -g @earendil-works/pi-coding-agent`, \
             or set its path in Settings › Agent defaults.",
            program.display()
        )
    })
}

/// Commands Treehouse sends, as JSON values.
pub mod command {
    use serde_json::{Value, json};

    pub fn prompt(message: &str, streaming: bool) -> Value {
        if streaming {
            json!({"type": "prompt", "message": message, "streamingBehavior": "steer"})
        } else {
            json!({"type": "prompt", "message": message})
        }
    }
    pub fn abort() -> Value {
        json!({"type": "abort"})
    }
    pub fn get_state() -> Value {
        json!({"type": "get_state"})
    }
    pub fn get_session_stats() -> Value {
        json!({"type": "get_session_stats"})
    }
    pub fn get_available_models() -> Value {
        json!({"type": "get_available_models"})
    }
    pub fn set_model(provider: &str, model_id: &str) -> Value {
        json!({"type": "set_model", "provider": provider, "modelId": model_id})
    }
    pub fn set_thinking_level(level: &str) -> Value {
        json!({"type": "set_thinking_level", "level": level})
    }
    pub fn get_last_assistant_text() -> Value {
        json!({"type": "get_last_assistant_text"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_responses_and_events() {
        let line = r#"{"id":"th-1","type":"response","command":"set_model","success":false,"error":"Model not found"}"#;
        let Some(PiMessage::Response(response)) = parse_line(line) else {
            panic!("expected a response");
        };
        assert_eq!(response.id.as_deref(), Some("th-1"));
        assert_eq!(response.command, "set_model");
        assert!(!response.success);
        assert_eq!(response.error.as_deref(), Some("Model not found"));

        let Some(PiMessage::Event(event)) = parse_line("{\"type\":\"agent_start\"}\r") else {
            panic!("expected an event");
        };
        assert_eq!(event["type"], "agent_start");
        assert!(parse_line("").is_none());
        assert!(parse_line("not json").is_none());
    }

    #[test]
    fn builds_launch_arguments() {
        let options = LaunchOptions {
            program: "pi".into(),
            cwd: "/tmp".into(),
            model: Some("anthropic/claude-opus-4-5".into()),
            thinking: Some("high".into()),
            tools: vec!["read".into(), "bash".into()],
            extensions: vec!["/x/treehouse.ts".into()],
            session_dir: None,
        };
        assert_eq!(
            options.args(),
            [
                "--mode", "rpc", "--model", "anthropic/claude-opus-4-5", "--thinking", "high",
                "--tools", "read,bash", "-e", "/x/treehouse.ts", "--no-session"
            ]
        );
    }
}
