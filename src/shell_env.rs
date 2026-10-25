//! The login shell's `PATH`.
//!
//! An app started from the macOS Dock or Finder, or from a Linux desktop
//! launcher, inherits a bare `PATH` without the folders the user's shell
//! profile adds (Homebrew, nvm, npm's global bin). `pi` lives in one of those,
//! and so does the `node` its script runs with. So at startup Treehouse asks
//! the login shell for its `PATH`, as editors do, and puts it in front.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const START: &str = "__treehouse_env_start__";
const END: &str = "__treehouse_env_end__";

/// Adds the login shell's `PATH` to this process's. Call before any threads
/// start: it changes the environment.
pub fn load_login_path() {
    if cfg!(windows) || std::env::var_os("TREEHOUSE_NO_SHELL_ENV").is_some() {
        return;
    }
    let Some(shell_path) = login_shell_path() else {
        return;
    };
    let current = std::env::var("PATH").unwrap_or_default();
    let merged = merge_paths(&shell_path, &current);
    // SAFETY: called at the start of `main`, before any other thread exists.
    unsafe { std::env::set_var("PATH", merged) };
}

fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh"
            } else {
                "/bin/sh"
            }
            .to_string()
        });
    // `env` prints the same way whatever the shell, fish included. The markers
    // skip anything the profile prints itself.
    let script = format!("echo {START}; /usr/bin/env; echo {END}");
    let mut child = Command::new(&shell)
        .args(["-l", "-i", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A profile that waits for input mustn't hold up the app.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut output = String::new();
    child.stdout.take()?.read_to_string(&mut output).ok()?;
    parse_path(&output)
}

/// The `PATH=` line between the markers.
fn parse_path(output: &str) -> Option<String> {
    let start = output.find(START)? + START.len();
    let end = start + output[start..].find(END)?;
    output[start..end]
        .lines()
        .find_map(|line| line.strip_prefix("PATH="))
        .map(str::to_owned)
        .filter(|path| !path.is_empty())
}

/// The shell's folders first, then any of ours it doesn't have.
fn merge_paths(shell: &str, current: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in shell.split(':').chain(current.split(':')) {
        if !part.is_empty() && !parts.contains(&part) {
            parts.push(part);
        }
    }
    parts.join(":")
}

#[cfg(test)]
mod tests {
    use super::{END, START, merge_paths, parse_path};

    #[test]
    fn reads_the_shell_path() {
        let output = format!(
            "Welcome!\n{START}\nHOME=/Users/you\nPATH=/opt/homebrew/bin:/usr/bin\n{END}\nbye\n"
        );
        assert_eq!(
            parse_path(&output).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(parse_path("PATH=/usr/bin"), None);
        assert_eq!(
            merge_paths("/opt/homebrew/bin:/usr/bin", "/usr/bin:/bin"),
            "/opt/homebrew/bin:/usr/bin:/bin"
        );
    }
}
