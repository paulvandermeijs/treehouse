//! Pi's models as the model picker lists them: favorites and recent first,
//! then a group per provider, filtered by what's typed.

use std::collections::HashSet;
use std::ops::Range;

use gpui_kit::{App, Global};
use serde_json::Value;

use crate::config::Config;
use crate::pi::auth::PROVIDERS;
use crate::pi::rpc::{LaunchOptions, PiClient, PiMessage, command};

/// Above this many models, only the current model's provider starts open.
const OPEN_ALL_UP_TO: usize = 20;
/// How many recent models the picker shows.
const RECENT_SHOWN: usize = 3;
/// How many recent models are remembered.
const RECENT_KEPT: usize = 10;

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub provider: String,
    pub id: String,
    /// The name from Pi's catalogue; unknown for a model only seen in a
    /// setting or a session file.
    pub name: Option<String>,
    pub context_window: Option<u64>,
}

/// One model is another whatever else is known about it.
impl PartialEq for ModelInfo {
    fn eq(&self, other: &Self) -> bool {
        self.provider == other.provider && self.id == other.id
    }
}

impl ModelInfo {
    pub fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            provider: value.get("provider")?.as_str()?.to_string(),
            id: value.get("id")?.as_str()?.to_string(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.trim().is_empty())
                .map(str::to_string),
            context_window: value.get("contextWindow").and_then(Value::as_u64),
        })
    }

    /// From a `provider/id` setting.
    pub fn parse(full_name: &str) -> Option<Self> {
        let (provider, id) = full_name.split_once('/')?;
        Some(Self {
            provider: provider.into(),
            id: id.into(),
            name: None,
            context_window: None,
        })
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }

    /// The maker, for providers that sell other makers' models
    /// (`~anthropic/claude-haiku-latest` is anthropic's).
    pub fn maker(&self) -> Option<&str> {
        let (maker, _) = self.id.split_once('/')?;
        Some(maker.trim_start_matches('~'))
    }

    /// A "latest" alias, which follows the maker's newest model.
    pub fn is_latest(&self) -> bool {
        self.id.ends_with("latest")
    }

    /// The name to show: Pi's, without the "Anthropic: " a reseller puts in
    /// front or the "latest" the tag shows; the id when there's no name.
    pub fn label(&self) -> String {
        let Some(name) = &self.name else {
            return self.id.rsplit('/').next().unwrap_or(&self.id).to_string();
        };
        let mut name = name.as_str();
        if self.maker().is_some()
            && let Some((_, rest)) = name.split_once(": ")
        {
            name = rest;
        }
        if self.is_latest() {
            for suffix in [" (latest)", " latest", " Latest"] {
                if let Some(rest) = name.strip_suffix(suffix) {
                    name = rest;
                    break;
                }
            }
        }
        name.trim().to_string()
    }

    /// How a closed picker names the model: its name, else its id.
    pub fn short_label(&self) -> String {
        if self.name.is_some() {
            self.label()
        } else {
            self.id.clone()
        }
    }
}

/// Puts a model picked in a chat first in the recent list.
pub fn remember(full_name: &str, cx: &mut App) {
    Config::update(cx, |c| {
        c.recent_models.retain(|m| m != full_name);
        c.recent_models.insert(0, full_name.to_string());
        c.recent_models.truncate(RECENT_KEPT);
    });
}

/// The provider's name as Settings › Providers shows it.
pub fn provider_label(provider: &str) -> String {
    PROVIDERS
        .iter()
        .find(|(id, _, _)| *id == provider)
        .map(|(_, label, _)| label.to_string())
        .unwrap_or_else(|| provider.to_string())
}

/// Providers in the order of Settings › Providers, then the rest by name.
fn provider_rank(provider: &str) -> (usize, String) {
    let ix = PROVIDERS
        .iter()
        .position(|(id, _, _)| *id == provider)
        .unwrap_or(PROVIDERS.len());
    (ix, provider.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Favorites,
    Recent,
    Provider(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Settings' "Pi's default" or "Same as the chat".
    First,
    Header {
        section: Section,
        label: String,
        count: Option<usize>,
        /// `None` when it can't be folded (Favorites, Recent, filtering).
        open: Option<bool>,
    },
    Maker(String),
    Model {
        model: ModelInfo,
        label: String,
        /// Matched parts of the label and of `provider/id`, in bytes.
        label_marks: Vec<Range<usize>>,
        id_marks: Vec<Range<usize>>,
        favorite: bool,
        current: bool,
    },
}

impl Row {
    /// Rows the selection lands on: models, the first option and the
    /// headers that fold.
    pub fn selectable(&self) -> bool {
        match self {
            Row::First | Row::Model { .. } => true,
            Row::Header { open, .. } => open.is_some(),
            Row::Maker(_) => false,
        }
    }
}

pub struct ListOptions<'a> {
    pub models: &'a [ModelInfo],
    pub favorites: &'a [String],
    /// Most recent first.
    pub recent: &'a [String],
    /// `None` when the first option is current, or there's no model.
    pub current: Option<&'a str>,
    pub query: &'a str,
    /// Groups folded or unfolded against their default.
    pub toggled: &'a HashSet<String>,
    pub first_option: bool,
}

pub fn rows(options: &ListOptions) -> Vec<Row> {
    let words: Vec<String> = options
        .query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let filtering = !words.is_empty();
    let model_row = |model: &ModelInfo| -> Option<Row> {
        let label = model.label();
        let full = model.full_name();
        let (label_lower, full_lower) = (label.to_lowercase(), full.to_lowercase());
        let mut label_marks = Vec::new();
        let mut id_marks = Vec::new();
        for word in &words {
            let in_label = find(&label_lower, word);
            let in_id = find(&full_lower, word);
            if in_label.is_none() && in_id.is_none() {
                return None;
            }
            label_marks.extend(in_label);
            id_marks.extend(in_id);
        }
        label_marks.sort_by_key(|r| r.start);
        id_marks.sort_by_key(|r| r.start);
        Some(Row::Model {
            favorite: options.favorites.contains(&full),
            current: options.current == Some(full.as_str()),
            model: model.clone(),
            label,
            label_marks,
            id_marks,
        })
    };

    let mut rows = Vec::new();
    if options.first_option && !filtering {
        rows.push(Row::First);
    }
    if !filtering {
        let find_model = |full: &String| options.models.iter().find(|m| m.full_name() == *full);
        let favorites: Vec<&ModelInfo> = options.favorites.iter().filter_map(find_model).collect();
        if !favorites.is_empty() {
            rows.push(header(Section::Favorites, "Favorites", None, None));
            rows.extend(favorites.into_iter().filter_map(model_row));
        }
        let recent: Vec<&ModelInfo> = options
            .recent
            .iter()
            .filter(|full| !options.favorites.contains(full))
            .filter_map(find_model)
            .take(RECENT_SHOWN)
            .collect();
        if !recent.is_empty() {
            rows.push(header(Section::Recent, "Recent", None, None));
            rows.extend(recent.into_iter().filter_map(model_row));
        }
    }

    let mut providers: Vec<&str> = Vec::new();
    for model in options.models {
        if !providers.contains(&model.provider.as_str()) {
            providers.push(&model.provider);
        }
    }
    providers.sort_by_key(|p| provider_rank(p));
    let current_provider = options
        .current
        .and_then(|c| c.split_once('/'))
        .map(|(p, _)| p);
    let open_all = options.models.len() <= OPEN_ALL_UP_TO;
    for provider in providers {
        let in_group: Vec<&ModelInfo> = options
            .models
            .iter()
            .filter(|m| m.provider == provider)
            .collect();
        let matched: Vec<(&ModelInfo, Row)> = in_group
            .iter()
            .filter_map(|m| Some((*m, model_row(m)?)))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let open = if filtering {
            None
        } else {
            let default = open_all || current_provider == Some(provider);
            Some(default != options.toggled.contains(provider))
        };
        let count = if filtering {
            matched.len()
        } else {
            in_group.len()
        };
        rows.push(header(
            Section::Provider(provider.to_string()),
            &provider_label(provider),
            Some(count),
            open,
        ));
        if open == Some(false) {
            continue;
        }
        // Resellers' models are split by maker; the rest keep Pi's order.
        if matched.iter().any(|(m, _)| m.maker().is_some()) {
            let mut makers: Vec<&str> = matched.iter().filter_map(|(m, _)| m.maker()).collect();
            makers.sort_unstable();
            makers.dedup();
            rows.extend(
                matched
                    .iter()
                    .filter(|(m, _)| m.maker().is_none())
                    .map(|(_, row)| row.clone()),
            );
            for maker in makers {
                rows.push(Row::Maker(maker.to_string()));
                rows.extend(
                    matched
                        .iter()
                        .filter(|(m, _)| m.maker() == Some(maker))
                        .map(|(_, row)| row.clone()),
                );
            }
        } else {
            rows.extend(matched.into_iter().map(|(_, row)| row));
        }
    }
    rows
}

fn header(section: Section, label: &str, count: Option<usize>, open: Option<bool>) -> Row {
    Row::Header {
        section,
        label: label.to_string(),
        count,
        open,
    }
}

/// Where `word` first occurs in `text`, both lowercase.
fn find(text: &str, word: &str) -> Option<Range<usize>> {
    text.find(word).map(|start| start..start + word.len())
}

/// The models of every provider with a key, as Pi reports them, for the
/// pickers that have no chat of their own (Settings).
#[derive(Default)]
pub struct Catalog {
    pub models: Vec<ModelInfo>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
}

impl Global for Catalog {}

impl Catalog {
    /// A chat's Pi reported its models; they're the same for every chat.
    pub fn store(models: Vec<ModelInfo>, cx: &mut App) {
        let catalog = cx.default_global::<Catalog>();
        catalog.models = models;
        catalog.loaded = true;
        catalog.error = None;
    }

    /// Asks a Pi of its own for the models, unless they're known or on
    /// their way. It runs without a session, tools or MCP servers, and
    /// is stopped as soon as it answers.
    pub fn load(cx: &mut App) {
        let catalog = cx.default_global::<Catalog>();
        if catalog.loaded || catalog.loading {
            return;
        }
        catalog.loading = true;
        let options = LaunchOptions {
            program: Config::global(cx).pi_command.clone().into(),
            cwd: dirs::home_dir().unwrap_or_else(std::env::temp_dir),
            approve: Some(false),
            ..Default::default()
        };
        let spawned = PiClient::spawn(&options);
        cx.spawn(async move |cx| {
            let result = match spawned {
                Ok((client, messages)) => {
                    client.send(command::get_available_models());
                    let mut result = Err("Pi stopped before listing its models".to_string());
                    while let Ok(message) = messages.recv().await {
                        match message {
                            PiMessage::Response(r) if r.command == "get_available_models" => {
                                result = if r.success {
                                    Ok(parse_models(&r.data))
                                } else {
                                    Err(r
                                        .error
                                        .unwrap_or_else(|| "Pi couldn't list its models".into()))
                                };
                                break;
                            }
                            PiMessage::Exited { .. } => break,
                            _ => {}
                        }
                    }
                    client.kill();
                    result
                }
                Err(err) => Err(format!("{err:#}")),
            };
            cx.update(|cx| {
                let catalog = cx.default_global::<Catalog>();
                catalog.loading = false;
                match result {
                    Ok(models) => {
                        catalog.models = models;
                        catalog.loaded = true;
                        catalog.error = None;
                    }
                    Err(err) => catalog.error = Some(err),
                }
                cx.refresh_windows();
            })
        })
        .detach();
    }
}

/// A `get_available_models` response's models.
pub fn parse_models(data: &Value) -> Vec<ModelInfo> {
    data["models"]
        .as_array()
        .map(|models| models.iter().filter_map(ModelInfo::from_json).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{ListOptions, ModelInfo, Row, Section, rows};
    use serde_json::json;
    use std::collections::HashSet;
    use std::ops::Range;

    fn model(provider: &str, id: &str, name: &str) -> ModelInfo {
        ModelInfo::from_json(&json!({"provider": provider, "id": id, "name": name})).unwrap()
    }

    fn catalog() -> Vec<ModelInfo> {
        vec![
            model("openrouter", "z-ai/glm-5.3-flash", "Z.AI: GLM 5.3 Flash"),
            model("anthropic", "claude-opus-4-5", "Claude Opus 4.5"),
            model(
                "openrouter",
                "anthropic/claude-haiku-4.5",
                "Anthropic: Claude Haiku 4.5",
            ),
            model("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5"),
            model(
                "openrouter",
                "~anthropic/claude-haiku-latest",
                "Anthropic: Claude Haiku Latest",
            ),
        ]
    }

    fn list(query: &str, favorites: &[&str], recent: &[&str], current: Option<&str>) -> Vec<Row> {
        let models = catalog();
        let favorites: Vec<String> = favorites.iter().map(|s| s.to_string()).collect();
        let recent: Vec<String> = recent.iter().map(|s| s.to_string()).collect();
        rows(&ListOptions {
            models: &models,
            favorites: &favorites,
            recent: &recent,
            current,
            query,
            toggled: &HashSet::new(),
            first_option: false,
        })
    }

    /// The rows as short strings, to compare whole lists.
    fn outline(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::First => "first".into(),
                Row::Header {
                    label, count, open, ..
                } => format!(
                    "# {label}{}{}",
                    count.map(|c| format!(" {c}")).unwrap_or_default(),
                    match open {
                        Some(true) => " v",
                        Some(false) => " >",
                        None => "",
                    }
                ),
                Row::Maker(maker) => format!("- {maker}"),
                Row::Model {
                    model,
                    label,
                    favorite,
                    current,
                    ..
                } => format!(
                    "{label} ({}){}{}",
                    model.full_name(),
                    if *favorite { " *" } else { "" },
                    if *current { " ok" } else { "" }
                ),
            })
            .collect()
    }

    #[test]
    fn labels_drop_reseller_prefix_and_latest() {
        let models = catalog();
        assert_eq!(models[0].label(), "GLM 5.3 Flash");
        assert_eq!(models[4].label(), "Claude Haiku");
        assert!(models[4].is_latest());
        assert_eq!(models[4].maker(), Some("anthropic"));
        assert_eq!(ModelInfo::parse("openai/gpt-5").unwrap().label(), "gpt-5");
    }

    #[test]
    fn favorites_and_recent_come_first_then_providers_in_settings_order() {
        let rows = list(
            "",
            &["openrouter/z-ai/glm-5.3-flash"],
            &[
                "openrouter/z-ai/glm-5.3-flash",
                "anthropic/claude-sonnet-4-5",
                "gone/model",
            ],
            Some("openrouter/z-ai/glm-5.3-flash"),
        );
        assert_eq!(
            outline(&rows),
            [
                "# Favorites",
                "GLM 5.3 Flash (openrouter/z-ai/glm-5.3-flash) * ok",
                "# Recent",
                "Claude Sonnet 4.5 (anthropic/claude-sonnet-4-5)",
                "# Anthropic 2 v",
                "Claude Opus 4.5 (anthropic/claude-opus-4-5)",
                "Claude Sonnet 4.5 (anthropic/claude-sonnet-4-5)",
                "# OpenRouter 3 v",
                "- anthropic",
                "Claude Haiku 4.5 (openrouter/anthropic/claude-haiku-4.5)",
                "Claude Haiku (openrouter/~anthropic/claude-haiku-latest)",
                "- z-ai",
                "GLM 5.3 Flash (openrouter/z-ai/glm-5.3-flash) * ok",
            ]
        );
    }

    #[test]
    fn long_lists_open_only_the_current_provider() {
        let mut models = catalog();
        for i in 0..20 {
            models.push(model("openai", &format!("gpt-{i}"), &format!("GPT {i}")));
        }
        let list = |toggled: &HashSet<String>| {
            rows(&ListOptions {
                models: &models,
                favorites: &[],
                recent: &[],
                current: Some("anthropic/claude-opus-4-5"),
                query: "",
                toggled,
                first_option: true,
            })
        };
        let headers = |rows: Vec<Row>| -> Vec<(Section, Option<bool>)> {
            rows.into_iter()
                .filter_map(|r| match r {
                    Row::Header { section, open, .. } => Some((section, open)),
                    _ => None,
                })
                .collect()
        };
        let provider = |p: &str| Section::Provider(p.into());
        let mut toggled = HashSet::new();
        let rows = list(&toggled);
        assert_eq!(rows[0], Row::First);
        assert_eq!(
            headers(rows),
            [
                (provider("anthropic"), Some(true)),
                (provider("openai"), Some(false)),
                (provider("openrouter"), Some(false)),
            ]
        );
        toggled.insert("openai".to_string());
        toggled.insert("anthropic".to_string());
        assert_eq!(
            headers(list(&toggled)),
            [
                (provider("anthropic"), Some(false)),
                (provider("openai"), Some(true)),
                (provider("openrouter"), Some(false)),
            ]
        );
    }

    #[test]
    fn filtering_matches_every_word_and_marks_it() {
        let rows = list(
            "haiku anth",
            &["openrouter/anthropic/claude-haiku-4.5"],
            &[],
            None,
        );
        assert_eq!(
            outline(&rows),
            [
                "# OpenRouter 2",
                "- anthropic",
                "Claude Haiku 4.5 (openrouter/anthropic/claude-haiku-4.5) *",
                "Claude Haiku (openrouter/~anthropic/claude-haiku-latest)",
            ]
        );
        let Row::Model {
            label_marks,
            id_marks,
            ..
        } = &rows[2]
        else {
            panic!("not a model row");
        };
        assert_eq!(label_marks.as_slice(), [Range { start: 7, end: 12 }]);
        assert_eq!(id_marks, &[11..15, 28..33]);
        assert!(list("mistral large", &[], &[], None).is_empty());
        // The provider counts as part of the id.
        assert_eq!(outline(&list("ROUTER glm", &[], &[], None)).len(), 3);
    }
}
