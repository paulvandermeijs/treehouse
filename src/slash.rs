//! Slash commands in the message box: Pi's skills and prompt templates, plus
//! `/compact` and `/new`, which Treehouse runs through RPC commands,
//! `/reload`, which restarts Pi because RPC mode has no reload, and the
//! worktree lifecycle's `/commit`, `/sync` and `/finish-branch`, which run a
//! skill.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    /// Run by Treehouse, not sent to Pi as a message.
    Builtin,
    Skill,
    Template,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    /// The name after the slash, such as `compact` or `skill:pdf-tools`.
    pub name: String,
    pub description: String,
    pub kind: CommandKind,
    /// Shown after the name, such as `[instructions]`.
    pub argument_hint: Option<String>,
    /// Why the command can't run here, shown in place of its kind.
    pub disabled: Option<String>,
}

/// The worktree lifecycle commands. Each runs a skill: the project's or the
/// user's own when Pi lists one by that name, else Treehouse's built-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Commit,
    Sync,
    FinishBranch,
    /// Writes `.agents/linked` and `.agents/prepare` for new worktrees.
    SetupWorktrees,
}

impl Lifecycle {
    pub const ALL: [Lifecycle; 4] = [
        Lifecycle::Commit,
        Lifecycle::Sync,
        Lifecycle::FinishBranch,
        Lifecycle::SetupWorktrees,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Lifecycle::Commit => "commit",
            Lifecycle::Sync => "sync",
            Lifecycle::FinishBranch => "finish-branch",
            Lifecycle::SetupWorktrees => "setup-worktrees",
        }
    }

    fn description(self, base: &str) -> String {
        match self {
            Lifecycle::Commit => "Commit the work so far, split into logical commits".into(),
            Lifecycle::Sync => format!("Bring in the latest {base}; Pi resolves conflicts"),
            Lifecycle::FinishBranch => format!("Commit, sync, test and merge into {base}"),
            Lifecycle::SetupWorktrees => {
                "Have Pi write .agents/linked and .agents/prepare for new worktrees".into()
            }
        }
    }

    /// Whether the command only makes sense on a branch with a base to
    /// sync with or merge into.
    pub fn needs_base(self) -> bool {
        matches!(self, Lifecycle::Sync | Lifecycle::FinishBranch)
    }

    /// The message Pi gets: the skill to run, with the user's instructions.
    pub fn prompt(self, instructions: &str, commands: &[SlashCommand]) -> String {
        let own = format!("skill:{}", self.name());
        let skill = if commands.iter().any(|c| c.name == own) {
            own
        } else {
            format!("skill:treehouse-{}", self.name())
        };
        match instructions.trim() {
            "" => format!("/{skill}"),
            text => format!("/{skill} {text}"),
        }
    }
}

/// Why a lifecycle command can't run, or `None` when it can.
pub fn lifecycle_unavailable(
    command: Lifecycle,
    trusted: bool,
    base: Option<&str>,
) -> Option<String> {
    if !trusted {
        Some("Read only".into())
    } else if command.needs_base() && base.is_none() {
        Some("On the base branch".into())
    } else {
        None
    }
}

/// The lifecycle commands for the slash menu. `base` is the branch the
/// worktree merges into; the main worktree has none. The kind says whether
/// the project's own skill runs instead of the built-in one.
pub fn lifecycle_commands(
    trusted: bool,
    base: Option<&str>,
    commands: &[SlashCommand],
) -> Vec<SlashCommand> {
    Lifecycle::ALL
        .iter()
        .map(|&command| {
            let own = format!("skill:{}", command.name());
            SlashCommand {
                name: command.name().into(),
                description: command.description(base.unwrap_or("the base branch")),
                kind: if commands.iter().any(|c| c.name == own) {
                    CommandKind::Skill
                } else {
                    CommandKind::Builtin
                },
                argument_hint: Some("[instructions]".into()),
                disabled: lifecycle_unavailable(command, trusted, base),
            }
        })
        .collect()
}

pub fn parse_lifecycle(message: &str) -> Option<(Lifecycle, &str)> {
    let message = message.trim();
    let (name, args) = match message.split_once(char::is_whitespace) {
        Some((name, args)) => (name, args.trim()),
        None => (message, ""),
    };
    let name = name.strip_prefix('/')?;
    Lifecycle::ALL
        .into_iter()
        .find(|c| c.name() == name)
        .map(|c| (c, args))
}

pub fn builtins() -> Vec<SlashCommand> {
    vec![
        SlashCommand {
            name: "compact".into(),
            description: "Compact the context, optionally with instructions for the summary".into(),
            kind: CommandKind::Builtin,
            argument_hint: Some("[instructions]".into()),
            disabled: None,
        },
        SlashCommand {
            name: "new".into(),
            description: "Start a new session in this worktree".into(),
            kind: CommandKind::Builtin,
            argument_hint: None,
            disabled: None,
        },
        SlashCommand {
            name: "reload".into(),
            description: "Restart Pi to pick up changed settings and MCP servers".into(),
            kind: CommandKind::Builtin,
            argument_hint: None,
            disabled: None,
        },
    ]
}

/// Reads the skills and templates from a `get_commands` response. Extension
/// commands are left out, and so are Treehouse's built-in skills, which the
/// lifecycle commands run.
pub fn parse_commands(data: &Value) -> Vec<SlashCommand> {
    let Some(commands) = data["commands"].as_array() else {
        return Vec::new();
    };
    commands
        .iter()
        .filter_map(|command| {
            let kind = match command["source"].as_str()? {
                "skill"
                    if command["name"]
                        .as_str()
                        .is_some_and(|n| n.starts_with("skill:treehouse-")) =>
                {
                    return None;
                }
                "skill" => CommandKind::Skill,
                "prompt" => CommandKind::Template,
                _ => return None,
            };
            let path = command["sourceInfo"]["path"].as_str();
            Some(SlashCommand {
                name: command["name"].as_str()?.to_string(),
                description: command["description"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                kind,
                // `get_commands` doesn't carry a template's argument hint;
                // its frontmatter does.
                argument_hint: match kind {
                    CommandKind::Template => path.and_then(read_argument_hint),
                    _ => None,
                },
                disabled: None,
            })
        })
        .collect()
}

fn read_argument_hint(path: &str) -> Option<String> {
    frontmatter_value(&std::fs::read_to_string(path).ok()?, "argument-hint")
}

fn frontmatter_value(source: &str, key: &str) -> Option<String> {
    let body = source.strip_prefix("---")?;
    let end = body.find("\n---")?;
    body[..end].lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim() == key).then(|| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
    })
}

/// The command name being typed: the text after a leading `/`, up to the
/// first space. `None` once arguments start or for an ordinary message.
pub fn typed_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('/')?;
    (!rest.contains(char::is_whitespace)).then_some(rest)
}

/// The commands matching what's typed: names starting with it first, then
/// names or descriptions containing it, each in the given order.
pub fn matches<'a>(commands: &'a [SlashCommand], query: &str) -> Vec<&'a SlashCommand> {
    let query = query.to_lowercase();
    let starts = |c: &&SlashCommand| {
        let name = c.name.to_lowercase();
        name.starts_with(&query)
            || name
                .strip_prefix("skill:")
                .is_some_and(|n| n.starts_with(&query))
    };
    let mut out: Vec<&SlashCommand> = commands.iter().filter(starts).collect();
    out.extend(commands.iter().filter(|c| {
        !starts(c)
            && (c.name.to_lowercase().contains(&query)
                || c.description.to_lowercase().contains(&query))
    }));
    out
}

/// A message that runs a built-in command instead of going to Pi.
#[derive(Debug, PartialEq, Eq)]
pub enum Builtin {
    Compact(Option<String>),
    New,
    Reload,
}

pub fn parse_builtin(message: &str) -> Option<Builtin> {
    let message = message.trim();
    let (name, args) = match message.split_once(char::is_whitespace) {
        Some((name, args)) => (name, args.trim()),
        None => (message, ""),
    };
    match name {
        "/compact" => Some(Builtin::Compact(
            (!args.is_empty()).then(|| args.to_string()),
        )),
        "/new" if args.is_empty() => Some(Builtin::New),
        "/reload" if args.is_empty() => Some(Builtin::Reload),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn command(name: &str, description: &str, kind: CommandKind) -> SlashCommand {
        SlashCommand {
            name: name.into(),
            description: description.into(),
            kind,
            argument_hint: None,
            disabled: None,
        }
    }

    #[test]
    fn keeps_skills_and_templates_only() {
        let data = json!({"commands": [
            {"name": "review", "description": "Review staged changes", "source": "prompt", "sourceInfo": {}},
            {"name": "skill:pdf-tools", "description": "Work with PDFs", "source": "skill", "sourceInfo": {}},
            {"name": "deploy", "source": "extension", "sourceInfo": {}},
            {"name": "skill:treehouse-commit", "description": "Commit", "source": "skill", "sourceInfo": {}},
        ]});
        let commands = parse_commands(&data);
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].kind, CommandKind::Template);
        assert_eq!(commands[1].name, "skill:pdf-tools");
    }

    #[test]
    fn reads_template_argument_hints() {
        let source = "---\ndescription: Review staged git changes\nargument-hint: \"[focus]\"\n---\nReview $@\n";
        assert_eq!(
            frontmatter_value(source, "argument-hint").as_deref(),
            Some("[focus]")
        );
        assert_eq!(frontmatter_value("Review $@", "argument-hint"), None);
    }

    #[test]
    fn finds_the_typed_name() {
        assert_eq!(typed_name("/"), Some(""));
        assert_eq!(typed_name("/com"), Some("com"));
        assert_eq!(typed_name("/compact keep the API notes"), None);
        assert_eq!(typed_name("hello /com"), None);
    }

    #[test]
    fn matches_prefixes_first() {
        let commands = vec![
            command("review", "Review staged changes", CommandKind::Template),
            command("skill:pdf-tools", "Work with PDF files", CommandKind::Skill),
            command("compact", "Compact the context", CommandKind::Builtin),
        ];
        let names = |q: &str| {
            matches(&commands, q)
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(""), ["review", "skill:pdf-tools", "compact"]);
        assert_eq!(names("pd"), ["skill:pdf-tools"]);
        assert_eq!(names("co"), ["compact"]);
        assert_eq!(names("staged"), ["review"]);
    }

    #[test]
    fn recognises_builtins() {
        assert_eq!(parse_builtin("/new"), Some(Builtin::New));
        assert_eq!(parse_builtin("/compact"), Some(Builtin::Compact(None)));
        assert_eq!(
            parse_builtin("/compact  keep the API notes "),
            Some(Builtin::Compact(Some("keep the API notes".into())))
        );
        assert_eq!(parse_builtin("/new thing"), None);
        assert_eq!(parse_builtin("/skill:pdf-tools x"), None);
    }

    #[test]
    fn lifecycle_runs_the_projects_own_skill_first() {
        assert_eq!(
            parse_lifecycle("/commit  focus on the parser "),
            Some((Lifecycle::Commit, "focus on the parser"))
        );
        assert_eq!(
            parse_lifecycle("/finish-branch"),
            Some((Lifecycle::FinishBranch, ""))
        );
        assert_eq!(parse_lifecycle("/committed"), None);

        let none: Vec<SlashCommand> = Vec::new();
        assert_eq!(Lifecycle::Sync.prompt("", &none), "/skill:treehouse-sync");
        let own = vec![command("skill:commit", "Our commits", CommandKind::Skill)];
        assert_eq!(
            Lifecycle::Commit.prompt("only the parser", &own),
            "/skill:commit only the parser"
        );
        let menu = lifecycle_commands(true, Some("main"), &own);
        assert_eq!(menu[0].kind, CommandKind::Skill);
        assert_eq!(menu[1].kind, CommandKind::Builtin);
        assert!(menu[2].description.contains("main"));
    }

    #[test]
    fn lifecycle_is_disabled_where_it_cant_run() {
        assert_eq!(
            lifecycle_unavailable(Lifecycle::Commit, false, Some("main")).as_deref(),
            Some("Read only")
        );
        assert_eq!(lifecycle_unavailable(Lifecycle::Commit, true, None), None);
        assert!(lifecycle_unavailable(Lifecycle::FinishBranch, true, None).is_some());
        // The main worktree is where new worktrees start from.
        assert_eq!(
            lifecycle_unavailable(Lifecycle::SetupWorktrees, true, None),
            None
        );
        assert_eq!(
            Lifecycle::SetupWorktrees.prompt("", &[]),
            "/skill:treehouse-setup-worktrees"
        );
        assert_eq!(
            lifecycle_unavailable(Lifecycle::Sync, true, Some("main")),
            None
        );
    }
}
