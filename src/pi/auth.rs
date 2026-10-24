//! Provider credentials, shared with Pi through its `auth.json`.

use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// The providers Settings › Providers lists, with their key variables.
pub const PROVIDERS: [(&str, &str, &str); 4] = [
    ("anthropic", "Anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OpenAI", "OPENAI_API_KEY"),
    ("google", "Google", "GEMINI_API_KEY"),
    ("openrouter", "OpenRouter", "OPENROUTER_API_KEY"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential {
    /// An API key in `auth.json`, shown masked.
    Stored(String),
    /// A sign-in Pi made through `/login`.
    OAuth,
    /// An environment variable Pi will read.
    Environment(&'static str),
    Missing,
}

impl Credential {
    pub fn is_set(&self) -> bool {
        !matches!(self, Credential::Missing)
    }
}

/// Pi's agent directory: `$PI_CODING_AGENT_DIR` or `~/.pi/agent`.
pub fn agent_dir() -> Option<PathBuf> {
    std::env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".pi").join("agent")))
}

fn read() -> Map<String, Value> {
    agent_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("auth.json")).ok())
        .and_then(|json| serde_json::from_str::<Value>(&json).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write(map: Map<String, Value>) -> std::io::Result<()> {
    let dir = agent_dir().ok_or_else(|| std::io::Error::other("no home folder"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("auth.json");
    std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(map))?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn credential(provider: &str) -> Credential {
    let map = read();
    match map.get(provider) {
        Some(entry) if entry["type"] == "api_key" => {
            Credential::Stored(mask(entry["key"].as_str().unwrap_or_default()))
        }
        Some(_) => Credential::OAuth,
        None => PROVIDERS
            .iter()
            .find(|(id, _, _)| *id == provider)
            .and_then(|(_, _, var)| std::env::var_os(var).map(|_| Credential::Environment(var)))
            .unwrap_or(Credential::Missing),
    }
}

pub fn save_key(provider: &str, key: &str) -> std::io::Result<()> {
    let mut map = read();
    map.insert(provider.to_string(), json!({"type": "api_key", "key": key.trim()}));
    write(map)
}

pub fn remove(provider: &str) -> std::io::Result<()> {
    let mut map = read();
    map.remove(provider);
    write(map)
}

/// `sk-ant-api03-abc…3f9a` → `sk-ant-api03-••••••••••3f9a`.
pub fn mask(key: &str) -> String {
    if key.starts_with('!') {
        return "Command: ".to_string() + &key[1..];
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "••••••••".into();
    }
    let prefix_len = key.rfind('-').map(|i| key[..=i].chars().count()).unwrap_or(3).min(chars.len() - 4);
    let prefix: String = chars[..prefix_len].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}••••••••••{suffix}")
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn masks_keys() {
        assert_eq!(mask("sk-ant-api03-abcdefghijklmnop3f9a"), "sk-ant-api03-••••••••••3f9a");
        assert_eq!(mask("short"), "••••••••");
        assert_eq!(mask("!pass show anthropic"), "Command: pass show anthropic");
    }
}
