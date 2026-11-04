//! Provider credentials, shared with Pi through its `auth.json`.

use serde_json::{Map, Value, json};
use std::path::PathBuf;

use crate::files::{self, Access};

/// The providers Settings › Providers offers, with their key variables, in
/// the order the page and the model picker list them. Only Pi's built-in
/// providers that take a single API key.
/// OpenCode's two plans share a variable, but Pi keeps a key per plan.
pub const PROVIDERS: [(&str, &str, &str); 18] = [
    ("anthropic", "Anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OpenAI", "OPENAI_API_KEY"),
    ("google", "Google", "GEMINI_API_KEY"),
    ("xai", "xAI (Grok)", "XAI_API_KEY"),
    ("mistral", "Mistral", "MISTRAL_API_KEY"),
    ("deepseek", "DeepSeek", "DEEPSEEK_API_KEY"),
    ("openrouter", "OpenRouter", "OPENROUTER_API_KEY"),
    ("opencode", "OpenCode Zen", "OPENCODE_API_KEY"),
    ("opencode-go", "OpenCode Go", "OPENCODE_API_KEY"),
    (
        "vercel-ai-gateway",
        "Vercel AI Gateway",
        "AI_GATEWAY_API_KEY",
    ),
    ("groq", "Groq", "GROQ_API_KEY"),
    ("cerebras", "Cerebras", "CEREBRAS_API_KEY"),
    ("together", "Together AI", "TOGETHER_API_KEY"),
    ("fireworks", "Fireworks", "FIREWORKS_API_KEY"),
    ("zai", "Z.ai", "ZAI_API_KEY"),
    ("moonshotai", "Moonshot AI", "MOONSHOT_API_KEY"),
    ("huggingface", "Hugging Face", "HF_TOKEN"),
    // Classifiers such as Jev, for auto mode's quick check.
    ("typesafe", "TypeSafe", "TYPESAFE_API_KEY"),
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
    let json = serde_json::to_string_pretty(&Value::Object(map))?;
    files::write(&dir.join("auth.json"), json, Access::Private)
}

pub fn credential(provider: &str) -> Credential {
    credential_in(&read(), provider)
}

/// Every provider's credential, from one read of `auth.json`.
pub fn credentials() -> Vec<(&'static str, Credential)> {
    let map = read();
    PROVIDERS
        .iter()
        .map(|&(id, _, _)| (id, credential_in(&map, id)))
        .collect()
}

fn credential_in(map: &Map<String, Value>, provider: &str) -> Credential {
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
    map.insert(
        provider.to_string(),
        json!({"type": "api_key", "key": key.trim()}),
    );
    write(map)
}

pub fn remove(provider: &str) -> std::io::Result<()> {
    let mut map = read();
    map.remove(provider);
    write(map)
}

/// `sk-ant-api03-abc…3f9a` → `sk-ant-api03-••••••••••3f9a`.
pub fn mask(key: &str) -> String {
    if let Some(command) = key.strip_prefix('!') {
        return format!("Command: {command}");
    }
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "••••••••".into();
    }
    let prefix_len = key
        .rfind('-')
        .map(|i| key[..=i].chars().count())
        .unwrap_or(3)
        .min(chars.len() - 4);
    let prefix: String = chars[..prefix_len].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}••••••••••{suffix}")
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn masks_keys() {
        assert_eq!(
            mask("sk-ant-api03-abcdefghijklmnop3f9a"),
            "sk-ant-api03-••••••••••3f9a"
        );
        assert_eq!(mask("short"), "••••••••");
        assert_eq!(mask("!pass show anthropic"), "Command: pass show anthropic");
    }
}
