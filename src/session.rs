//! One Pi session: a worktree's chat or a sub-agent.
//!
//! The session owns the Pi process, turns its events into a [`Transcript`],
//! applies the approval policy, and reports sub-agent requests to the
//! workspace, which owns the sub-agent sessions.

use gpui_kit::component::text::TextViewState;
use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, SharedString, Task};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::approval::{AllowedPattern, Decision, Policy, ToolCallRequest, propose_pattern};
use crate::config::Config;
use crate::pi::rpc::{LaunchOptions, PiClient, PiMessage, command, str_field};
use crate::transcript::{Effect, Item, ItemId, RunState, SignInState, ToolStatus, Transcript};

/// Sidebar and card status, most urgent first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentStatus {
    NeedsInput,
    Failed,
    Working,
    Finished,
    Idle,
}

impl AgentStatus {
    /// A worktree shows its most urgent status, so a collapsed worktree
    /// still says a sub-agent is waiting.
    pub fn most_urgent(statuses: impl IntoIterator<Item = AgentStatus>) -> AgentStatus {
        statuses.into_iter().min().unwrap_or(AgentStatus::Idle)
    }
}

#[derive(Clone, Debug)]
pub enum SessionKind {
    Worktree,
    SubAgent {
        title: SharedString,
        task: String,
        /// The parent's `subagent` tool call that started it.
        tool_call_id: String,
    },
}

/// A tool call waiting for the user's decision.
#[derive(Clone, Debug)]
pub struct PendingApproval {
    pub request_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: Value,
    /// The "Always allow" pattern proposed for a bash command.
    pub pattern: Option<String>,
    pub asked: Instant,
}

/// A merge into the base branch that passed its checks and waits for the
/// user. It always asks, whatever the approval setting: it changes a branch
/// outside the worktree.
#[derive(Clone, Debug)]
pub struct PendingMerge {
    pub request_id: String,
    pub tool_call_id: String,
    pub branch: String,
    pub base: String,
    pub plan: crate::git::MergePlan,
    /// Pi's message for the commit if the user squashes the branch.
    pub squash_message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeChoice {
    FastForward,
    Squash,
    Deny,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub provider: String,
    pub id: String,
    pub context_window: Option<u64>,
}

impl ModelInfo {
    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            provider: value.get("provider")?.as_str()?.to_string(),
            id: value.get("id")?.as_str()?.to_string(),
            context_window: value.get("contextWindow").and_then(Value::as_u64),
        })
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

pub enum SessionEvent {
    /// The parent asked for a sub-agent; the workspace starts it.
    SubAgentRequested {
        request_id: String,
        tool_call_id: String,
        title: String,
        task: String,
    },
    /// Pi's `treehouse_merge` tool wants the worktree merged into its base;
    /// the workspace checks it's safe.
    MergeRequested {
        request_id: String,
        tool_call_id: String,
        squash_message: String,
    },
    /// The user approved a merge; the workspace carries it out.
    MergeApproved(PendingMerge, MergeChoice),
    /// Pi's `treehouse_worktree` tool wants a new worktree, perhaps with a
    /// task for its chat; the workspace creates it.
    WorktreeRequested {
        request_id: String,
        branch: String,
        task: String,
    },
    /// A turn or a `!` command ended; the worktree's diff, and the
    /// project's worktrees, may have changed.
    TurnEnded,
    StatusChanged,
}

pub struct Session {
    pub kind: SessionKind,
    pub cwd: PathBuf,
    pub project: PathBuf,
    client: Option<PiClient>,
    transcript: Transcript,
    /// Each reply's Markdown view and how much of its text it has. The text
    /// streams in as deltas; once the reply ends its tables get their final
    /// alignment, and the length is set to `usize::MAX` to mark that done.
    texts: HashMap<ItemId, (Entity<TextViewState>, usize)>,
    pub approvals: Vec<PendingApproval>,
    pub merge: Option<PendingMerge>,
    /// The worktree's HEAD as last seen during the current run, and the time
    /// (Unix seconds) the run started, for the cards listing its commits.
    head_seen: Option<(String, u64)>,
    /// Sub-agent requests waiting on a child session's report.
    pub subagent_requests: HashMap<String, String>,
    /// Set once the user has opened a finished turn.
    viewed: bool,
    pub model: Option<ModelInfo>,
    pub models: Vec<ModelInfo>,
    pub thinking: String,
    pub context_tokens: Option<u64>,
    /// Pi's skills and prompt templates for the slash menu.
    pub commands: Vec<crate::slash::SlashCommand>,
    /// Whether the project is trusted. An untrusted project runs Pi
    /// read-only: the read tool only, without the project's own Pi resources.
    pub trusted: bool,
    /// The branch this worktree syncs with and merges into. The main
    /// worktree and sub-agents have none, so they get no lifecycle skills.
    pub base: Option<String>,
    /// Pi's saved session, so a restart continues the same conversation.
    session_file: Option<PathBuf>,
    /// Whether the saved session's chat has been read back in; see
    /// `restore_history`.
    restored: bool,
    pub started: Instant,
    pub start_error: Option<String>,
    /// Why Pi stopped unexpectedly. Live state rather than history: there's
    /// no Pi left to save it to.
    pub exited: Option<String>,
    /// Entries for Pi's session file waiting for Pi to run.
    unsaved: Vec<(&'static str, Value)>,
    _reader: Option<Task<()>>,
    /// Messages sent while the worktree's setup runs. Pi starts once it
    /// ends, so it finds the worktree ready.
    pub setup_queue: Vec<String>,
    /// Setup that waits for the project to be trusted.
    pub setup_waiting: bool,
    setup_stop: Option<SetupStop>,
    _setup: Option<Task<()>>,
}

/// Reaches the running setup script to stop it.
#[derive(Clone)]
struct SetupStop {
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

enum SetupMessage {
    Links(Vec<crate::setup::Link>),
    Output(String),
    Done {
        exit_code: Option<i64>,
        cancelled: bool,
        error: Option<String>,
    },
}

impl EventEmitter<SessionEvent> for Session {}

/// Pi's tools that reach MCP tools.
const MCP_DISCOVERY: [&str; 2] = ["codemode", "tool_search"];

/// Tells Pi why it can only read while the folder isn't trusted.
const READ_ONLY_NOTE: &str = "This folder isn't trusted yet, so you only have the read tool: you can't create, edit or delete files or run commands. If the user asks for a change, don't attempt it or claim to have made it. Explain that they need to trust the folder first (the \"Read only\" button at the top of the chat), and say what you would change.";

/// Tells Pi which branch the lifecycle skills sync with and merge into.
fn base_note(base: &str) -> String {
    format!("This worktree's base branch is {base}.")
}

impl Session {
    pub fn new(kind: SessionKind, cwd: PathBuf, project: PathBuf, cx: &App) -> Self {
        let config = Config::global(cx);
        let model = match &kind {
            SessionKind::SubAgent { .. } => config.subagent_model.clone().or(config.model.clone()),
            SessionKind::Worktree => config.model.clone(),
        };
        // A worktree's chat continues where it was left.
        let session_file = match &kind {
            SessionKind::Worktree => crate::history::saved(&cwd).map(|saved| saved.file),
            // Set by the workspace when it restores one.
            SessionKind::SubAgent { .. } => None,
        };
        Self {
            transcript: Transcript::new(cwd.clone()),
            kind,
            cwd,
            project,
            client: None,
            texts: HashMap::new(),
            approvals: Vec::new(),
            merge: None,
            head_seen: None,
            subagent_requests: HashMap::new(),
            viewed: true,
            model: model.and_then(|m| {
                let (provider, id) = m.split_once('/')?;
                Some(ModelInfo {
                    provider: provider.into(),
                    id: id.into(),
                    context_window: None,
                })
            }),
            models: Vec::new(),
            thinking: config.thinking.clone(),
            context_tokens: None,
            commands: Vec::new(),
            trusted: false,
            base: None,
            session_file,
            restored: false,
            started: Instant::now(),
            start_error: None,
            exited: None,
            unsaved: Vec::new(),
            _reader: None,
            setup_queue: Vec::new(),
            setup_waiting: false,
            setup_stop: None,
            _setup: None,
        }
    }

    pub fn title(&self) -> SharedString {
        match &self.kind {
            SessionKind::SubAgent { title, .. } => title.clone(),
            SessionKind::Worktree => self
                .cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
                .into(),
        }
    }

    pub fn is_subagent(&self) -> bool {
        matches!(self.kind, SessionKind::SubAgent { .. })
    }

    pub fn items(&self) -> &[Item] {
        self.transcript.items()
    }

    pub fn transcript_run_started(&self) -> Option<Instant> {
        self.transcript.run_started
    }

    pub fn transcript_settled_at(&self) -> Option<Instant> {
        self.transcript.settled_at
    }

    pub fn is_running(&self) -> bool {
        self.transcript.run == RunState::Running
    }

    pub fn status(&self) -> AgentStatus {
        if !self.approvals.is_empty() || self.merge.is_some() || self.waiting_sign_in().is_some() {
            AgentStatus::NeedsInput
        } else if self.transcript.run == RunState::Running || self.transcript.shell_running() {
            AgentStatus::Working
        } else if self.transcript.run == RunState::Failed || self.start_error.is_some() {
            AgentStatus::Failed
        } else if !self.viewed {
            AgentStatus::Finished
        } else {
            AgentStatus::Idle
        }
    }

    /// The user opened this session: a finished turn turns Idle.
    pub fn mark_viewed(&mut self, cx: &mut Context<Self>) {
        if !self.viewed && !self.is_running() {
            self.viewed = true;
            cx.emit(SessionEvent::StatusChanged);
            cx.notify();
        }
    }

    pub fn tool_call_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| matches!(item, Item::Tool(_)))
            .count()
    }

    /// The rendered markdown of an assistant text item.
    pub fn text_view(&self, id: ItemId) -> Option<&Entity<TextViewState>> {
        self.texts.get(&id).map(|(state, _)| state)
    }

    /// The last thing the assistant said, used as a sub-agent's report.
    pub fn last_assistant_text(&self) -> Option<String> {
        self.items().iter().rev().find_map(|item| match item {
            Item::Text { text, .. } if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        })
    }

    /// Shows the saved session's chat. Done when the chat is first opened or
    /// Pi starts, rather than for every worktree at launch, since session
    /// files can be large.
    /// Continues a saved session: shown once the chat is opened, resumed
    /// once Pi starts.
    pub fn resume(&mut self, file: PathBuf) {
        self.session_file = Some(file);
    }

    pub fn restore_history(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.restored, true) {
            return;
        }
        let Some(file) = &self.session_file else {
            return;
        };
        let entries = crate::history::load(file);
        self.transcript.restore(&entries);
        self.context_tokens = entries
            .iter()
            .rev()
            .filter(|e| e["message"]["role"] == "assistant")
            .find_map(|e| e["message"]["usage"]["totalTokens"].as_u64())
            .filter(|tokens| *tokens > 0);
        // "Task from Pi · 12m ago" and sub-agent cards count from the start.
        if let Some(first) = entries
            .iter()
            .find_map(|e| e["message"]["timestamp"].as_u64())
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let ago = now.saturating_sub(std::time::Duration::from_millis(first));
            self.started = Instant::now().checked_sub(ago).unwrap_or(self.started);
        }
        if let Some(last) = entries
            .iter()
            .rev()
            .find_map(|e| e["message"]["timestamp"].as_u64())
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let ago = now.saturating_sub(std::time::Duration::from_millis(last));
            self.transcript.settled_at = Instant::now().checked_sub(ago);
        }
        // Until Pi starts, show the model the chat last used rather than the
        // default: a default like `z-ai/glm-5.3-flash` can resolve to another
        // provider in Pi, which only the session records.
        if let Some(model) = entries.iter().rev().find_map(|e| {
            let (provider, id) = match e["type"].as_str() {
                Some("model_change") => (e["provider"].as_str()?, e["modelId"].as_str()?),
                Some("message") if e["message"]["role"] == "assistant" => (
                    e["message"]["provider"].as_str()?,
                    e["message"]["model"].as_str()?,
                ),
                _ => return None,
            };
            Some(ModelInfo {
                provider: provider.into(),
                id: id.into(),
                context_window: None,
            })
        }) {
            self.model = Some(model);
        }
        self.sync_texts(cx);
        cx.notify();
    }

    fn ensure_started(&mut self, cx: &mut Context<Self>) -> bool {
        if self.client.is_some() {
            return true;
        }
        self.restore_history(cx);
        let config = Config::global(cx);
        let extension = match crate::pi::extension_path() {
            Ok(path) => path,
            Err(err) => {
                self.fail_to_start(
                    format!("Couldn't write the Treehouse extension for Pi: {err}"),
                    cx,
                );
                return false;
            }
        };
        // Untrusted, Pi can only read, so the lifecycle commands can't run.
        let skills = if self.trusted && !self.is_subagent() {
            match crate::pi::skill_paths() {
                Ok(paths) => paths,
                Err(err) => {
                    self.fail_to_start(
                        format!("Couldn't write Treehouse's skills for Pi: {err}"),
                        cx,
                    );
                    return false;
                }
            }
        } else {
            Vec::new()
        };
        let options = LaunchOptions {
            program: config.pi_command.clone().into(),
            cwd: self.cwd.clone(),
            model: self.model.as_ref().map(ModelInfo::full_name),
            thinking: Some(self.thinking.clone()),
            // `--tools` replaces Pi's whole selection, so the bridge's own tool
            // has to be named. Sub-agents don't get it: no nested sub-agents.
            // MCP tools are only reachable through `codemode` and
            // `tool_search`; Pi activates them when a server needs them, and
            // the tools a script calls still go through approval.
            tools: if self.trusted {
                config
                    .tools
                    .iter()
                    .cloned()
                    .chain((!self.is_subagent()).then(|| "subagent".to_string()))
                    .chain(self.base.is_some().then(|| "treehouse_merge".to_string()))
                    .chain((!self.is_subagent()).then(|| "treehouse_worktree".to_string()))
                    .chain(MCP_DISCOVERY.iter().map(|t| t.to_string()))
                    .collect()
            } else {
                vec!["read".to_string()]
            },
            extensions: vec![extension],
            skills,
            session_dir: Some(Config::data_dir().join("sessions")),
            session_file: self.session_file.clone(),
            approve: Some(self.trusted),
            // Without this, a model asked to change something tends to try
            // tools it doesn't have and then report success anyway.
            // Read-only means no MCP servers either: their tools can change
            // things outside the folder.
            mcp: self.trusted,
            system_note: if self.trusted {
                self.base.as_ref().map(|base| base_note(base))
            } else {
                Some(READ_ONLY_NOTE.to_string())
            },
        };
        match PiClient::spawn(&options) {
            Ok((client, messages)) => {
                client.send(command::get_state());
                client.send(command::get_available_models());
                client.send(command::get_commands());
                self.client = Some(client);
                self.start_error = None;
                self.exited = None;
                // Before any message, so they keep their place in the chat.
                self.flush_saves();
                self._reader = Some(cx.spawn(async move |this, cx| {
                    while let Ok(message) = messages.recv().await {
                        let alive = this.update(cx, |this, cx| this.handle(message, cx)).is_ok();
                        if !alive {
                            break;
                        }
                    }
                }));
                true
            }
            Err(err) => {
                self.fail_to_start(format!("{err:#}"), cx);
                false
            }
        }
    }

    /// Shown as live state: with no Pi there's nowhere to save it.
    fn fail_to_start(&mut self, error: String, cx: &mut Context<Self>) {
        self.start_error = Some(error);
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    /// Sends a message; while Pi is working it steers the current run.
    /// `/compact` and `/new` run as commands instead.
    pub fn send(&mut self, message: &str, cx: &mut Context<Self>) {
        let message = message.trim();
        if message.is_empty() {
            return;
        }
        if self.transcript.setup_running() {
            self.setup_queue.push(message.to_string());
            return cx.notify();
        }
        if !self.ensure_started(cx) {
            return;
        }
        if let Some(command) = message.strip_prefix('!') {
            let (command, excluded) = match command.strip_prefix('!') {
                Some(command) => (command, true),
                None => (command, false),
            };
            return self.run_shell(command.trim(), excluded, cx);
        }
        match crate::slash::parse_builtin(message) {
            Some(crate::slash::Builtin::Compact(instructions)) => {
                return self.compact(instructions.as_deref(), cx);
            }
            Some(crate::slash::Builtin::New) => return self.new_session(cx),
            Some(crate::slash::Builtin::Reload) => {
                return self.restart(
                    "Pi restarted · settings, skills and MCP servers reloaded",
                    cx,
                );
            }
            None => {}
        }
        let streaming = self.is_running();
        // A lifecycle command runs its skill; the chat still shows what was typed.
        let lifecycle = crate::slash::parse_lifecycle(message).filter(|_| !self.is_subagent());
        let prompt = match lifecycle {
            Some((lifecycle, instructions)) => {
                // The chat says why before sending; see `ChatView::send`.
                if crate::slash::lifecycle_unavailable(
                    lifecycle,
                    self.trusted,
                    self.base.as_deref(),
                )
                .is_some()
                {
                    return;
                }
                lifecycle.prompt(instructions, &self.commands)
            }
            None => message.to_string(),
        };
        if !streaming && !self.is_subagent() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.head_seen = crate::git::head(&self.cwd).map(|head| (head, now));
        }
        if let Some(client) = &self.client {
            // `/mcp …` is an extension command: Pi runs it without a message.
            if !message.starts_with("/mcp") {
                self.transcript.expect_user_text(message);
            }
            client.send(command::prompt(&prompt, streaming));
        }
        self.viewed = true;
        cx.notify();
    }

    /// Runs a command the user typed after `!` in the worktree, through Pi so
    /// that it can see the output.
    fn run_shell(&mut self, command: &str, excluded: bool, cx: &mut Context<Self>) {
        if command.is_empty() {
            return;
        }
        if let Some(client) = &self.client {
            let request_id = client.send(command::bash(command, excluded));
            self.transcript.start_shell(request_id, command, excluded);
            cx.emit(SessionEvent::StatusChanged);
        }
        self.viewed = true;
        cx.notify();
    }

    /// Stops the running `!` command.
    pub fn stop_shell(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.client {
            client.send(command::abort_bash());
        }
        cx.notify();
    }

    /// Whether the worktree's `.agents/` setup is running.
    pub fn setup_running(&self) -> bool {
        self.transcript.setup_running()
    }

    pub fn shell_running(&self) -> bool {
        self.transcript.shell_running()
    }

    fn compact(&mut self, instructions: Option<&str>, cx: &mut Context<Self>) {
        if let Some(client) = &self.client {
            client.send(command::compact(instructions));
            self.transcript.compacting = true;
        }
        cx.notify();
    }

    /// Starts a new Pi session; the chat clears once Pi confirms.
    fn new_session(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.client {
            client.send(command::new_session());
        }
        cx.notify();
    }

    /// Fetches the slash menu's commands, starting Pi if it isn't running.
    pub fn load_commands(&mut self, cx: &mut Context<Self>) {
        if self.ensure_started(cx)
            && let Some(client) = &self.client
        {
            client.send(command::get_commands());
        }
    }

    /// Prepares a new worktree with the project's `.agents/linked` and
    /// `.agents/prepare`, as a command row in the chat. In a project that
    /// isn't trusted yet it waits: they're the project's own code.
    pub fn run_setup(&mut self, cx: &mut Context<Self>) {
        let worktree = self.cwd.clone();
        let root = self.project.clone();
        if !crate::setup::has_setup(&worktree) || self.transcript.setup_running() {
            return;
        }
        if !self.trusted {
            self.setup_waiting = true;
            return cx.notify();
        }
        self.setup_waiting = false;
        self.transcript.start_setup(crate::setup::title(&worktree));
        let stop = SetupStop {
            child: Default::default(),
            stopped: Default::default(),
        };
        self.setup_stop = Some(stop.clone());
        let (tx, rx) = smol::channel::unbounded();
        cx.background_spawn(async move { run_setup_blocking(&root, &worktree, stop, tx) })
            .detach();
        self._setup = Some(cx.spawn(async move |this, cx| {
            while let Ok(message) = rx.recv().await {
                if this
                    .update(cx, |this, cx| this.on_setup(message, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    fn on_setup(&mut self, message: SetupMessage, cx: &mut Context<Self>) {
        match message {
            SetupMessage::Links(links) => {
                if let Some(run) = self.transcript.setup_run() {
                    run.setup = Some(links);
                }
            }
            SetupMessage::Output(line) => {
                if let Some(run) = self.transcript.setup_run() {
                    run.output.push_str(&line);
                    run.output.push('\n');
                }
            }
            SetupMessage::Done {
                exit_code,
                cancelled,
                error,
            } => {
                self.transcript.finish_setup(exit_code, cancelled, error);
                self.flush_saves();
                self.setup_stop = None;
                cx.emit(SessionEvent::StatusChanged);
                for message in std::mem::take(&mut self.setup_queue) {
                    self.send(&message, cx);
                }
            }
        }
        cx.notify();
    }

    /// Stops the setup script; Pi can start.
    pub fn stop_setup(&mut self, _: &mut Context<Self>) {
        if let Some(stop) = &self.setup_stop {
            stop.stopped
                .store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(child) = stop.child.lock().ok().as_mut().and_then(|c| c.as_mut()) {
                crate::setup::stop(child);
            }
        }
    }

    /// Hands a failed setup to Pi to fix.
    pub fn ask_to_fix_setup(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.items().iter().rev().find_map(|item| match item {
            Item::Shell(run) if run.setup.is_some() => Some(run.clone()),
            _ => None,
        }) else {
            return;
        };
        let tail: Vec<&str> = run.output.lines().rev().take(40).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        let why = match (&run.error, run.exit_code) {
            (Some(error), _) => error.clone(),
            (None, Some(code)) => format!("exit code {code}"),
            _ => "an error".to_string(),
        };
        self.send(
            &format!(
                "The worktree setup (`{}`) failed with {why}. Please find the cause and fix it so a new worktree can be prepared. Its output:\n\n```\n{}\n```",
                run.command,
                tail.join("\n")
            ),
            cx,
        );
    }

    /// Applies a trust decision. A running Pi restarts on the same saved
    /// session, since its tools and project resources are fixed at launch.
    pub fn set_trusted(&mut self, trusted: bool, cx: &mut Context<Self>) {
        if self.trusted == trusted {
            return;
        }
        self.trusted = trusted;
        if trusted && self.setup_waiting {
            self.run_setup(cx);
        }
        if self.client.is_none() {
            return cx.notify();
        }
        self.restart(
            if trusted {
                "Folder trusted · Pi restarted with full access"
            } else {
                "Read-only · Pi restarted with the read tool only"
            },
            cx,
        );
    }

    /// Stops Pi and starts it again on the same session file, so it reads
    /// its settings, skills, extensions and MCP servers afresh.
    fn restart(&mut self, notice: &str, cx: &mut Context<Self>) {
        self._reader = None;
        self.client = None;
        self.approvals.clear();
        self.merge = None;
        self.transcript.abandon_shells();
        self.transcript
            .apply(&serde_json::json!({"type": "agent_settled"}));
        self.transcript.saved_notice(notice);
        self.ensure_started(cx);
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    /// The sign-in Pi is waiting on, with the request to answer.
    pub fn waiting_sign_in(&self) -> Option<&crate::transcript::SignIn> {
        self.items().iter().rev().find_map(|item| match item {
            Item::SignIn(sign_in)
                if sign_in.state == SignInState::Waiting && sign_in.request_id.is_some() =>
            {
                Some(sign_in)
            }
            _ => None,
        })
    }

    /// Starts a sign-in to an MCP server through Pi's `/mcp login`.
    pub fn sign_in(&mut self, server: &str, cx: &mut Context<Self>) {
        self.transcript.retry_sign_in(server);
        self.transcript.sign_in(server);
        self.send(&crate::mcp::login_command(server), cx);
    }

    /// Hands Pi the URL the browser was redirected to, or cancels the sign-in.
    pub fn answer_sign_in(&mut self, redirect: Option<String>, cx: &mut Context<Self>) {
        let Some(request_id) = self.waiting_sign_in().and_then(|s| s.request_id.clone()) else {
            return;
        };
        self.answer(&request_id, redirect.as_deref());
        if let Some(sign_in) = self.transcript.waiting_sign_in(None) {
            sign_in.request_id = None;
        }
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    pub fn is_compacting(&self) -> bool {
        self.transcript.compacting
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(client) = &self.client {
            client.send(command::abort());
        }
        for approval in std::mem::take(&mut self.approvals) {
            self.answer(&approval.request_id, Some("deny:Stopped by the user"));
        }
        if let Some(merge) = self.merge.take() {
            self.answer_merge(&merge.request_id, "Stopped by the user", None);
        }
        cx.notify();
    }

    pub fn set_model(&mut self, model: &ModelInfo, cx: &mut Context<Self>) {
        let changed = self.model.as_ref() != Some(model);
        self.model = Some(model.clone());
        if let Some(client) = &self.client {
            client.send(command::set_model(&model.provider, &model.id));
            if changed {
                self.transcript
                    .push_notice(format!("Model changed to {}", model.id));
            }
        }
        cx.notify();
    }

    pub fn set_thinking(&mut self, level: &str, cx: &mut Context<Self>) {
        self.thinking = level.to_string();
        if let Some(client) = &self.client {
            client.send(command::set_thinking_level(level));
        }
        cx.notify();
    }

    /// Fetches the model list, starting Pi if it isn't running yet.
    pub fn load_models(&mut self, cx: &mut Context<Self>) {
        if self.ensure_started(cx)
            && let Some(client) = &self.client
        {
            client.send(command::get_available_models());
        }
    }

    fn answer(&self, request_id: &str, value: Option<&str>) {
        if let Some(client) = &self.client {
            client.respond_ui(request_id, value);
        }
    }

    /// The user decided on a pending tool call.
    pub fn resolve_approval(
        &mut self,
        request_id: &str,
        choice: ApprovalChoice,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self
            .approvals
            .iter()
            .position(|a| a.request_id == request_id)
        else {
            return;
        };
        let approval = self.approvals.remove(ix);
        let answer = match &choice {
            ApprovalChoice::Run => "allow".to_string(),
            ApprovalChoice::AlwaysAllow(pattern) => {
                let pattern = pattern.clone();
                let project = self.project.clone();
                Config::update(cx, |config| {
                    let allowed = AllowedPattern {
                        pattern,
                        project: Some(project),
                    };
                    if !config.allowed.contains(&allowed) {
                        config.allowed.push(allowed);
                    }
                });
                "allow".to_string()
            }
            ApprovalChoice::Deny(reason) => format!("deny:{reason}"),
        };
        if let Some(tool) = self.transcript.tool_mut(&approval.tool_call_id) {
            tool.status = match choice {
                ApprovalChoice::Deny(_) => ToolStatus::Denied,
                _ => ToolStatus::Running,
            };
        }
        self.answer(request_id, Some(&answer));
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    /// A sub-agent this session started has reported back, or was stopped.
    pub fn finish_subagent(
        &mut self,
        request_id: &str,
        report: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.subagent_requests.remove(request_id);
        self.answer(request_id, report.as_deref());
        cx.notify();
    }

    /// Answers the merge tool. Without `merged` the tool fails with `text`.
    fn answer_merge(&self, request_id: &str, text: &str, merged: Option<Value>) {
        let outcome = serde_json::json!({"text": text, "merged": merged});
        self.answer(request_id, Some(&outcome.to_string()));
    }

    /// Answers the worktree tool: the new worktree, or why there isn't one.
    pub fn finish_worktree(
        &mut self,
        request_id: &str,
        outcome: Result<(String, Value), String>,
        cx: &mut Context<Self>,
    ) {
        let outcome = match outcome {
            Ok((text, worktree)) => serde_json::json!({"text": text, "worktree": worktree}),
            Err(reason) => serde_json::json!({"text": reason, "worktree": null}),
        };
        self.answer(request_id, Some(&outcome.to_string()));
        cx.notify();
    }

    /// Starts this worktree's chat on a task another chat handed over. It
    /// waits for the worktree's setup, as any message does.
    pub fn hand_over(&mut self, from: String, task: String, cx: &mut Context<Self>) {
        self.transcript.hand_over(from, task.clone());
        self.send(&task, cx);
    }

    /// The merge passed Treehouse's checks: show its approval card.
    pub fn ask_merge(&mut self, merge: PendingMerge, cx: &mut Context<Self>) {
        if let Some(tool) = self.transcript.tool_mut(&merge.tool_call_id) {
            tool.status = ToolStatus::AwaitingApproval;
        }
        self.merge = Some(merge);
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    /// The user decided on the waiting merge.
    pub fn resolve_merge(&mut self, choice: MergeChoice, cx: &mut Context<Self>) {
        let Some(merge) = self.merge.take() else {
            return;
        };
        let denied = choice == MergeChoice::Deny;
        if let Some(tool) = self.transcript.tool_mut(&merge.tool_call_id) {
            tool.status = if denied {
                ToolStatus::Denied
            } else {
                ToolStatus::Running
            };
        }
        if denied {
            self.answer_merge(&merge.request_id, "Denied by the user in Treehouse", None);
        } else {
            cx.emit(SessionEvent::MergeApproved(merge, choice));
        }
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    /// Reports the merge's outcome, or why it was refused, to Pi.
    pub fn finish_merge(
        &mut self,
        request_id: &str,
        outcome: Result<(String, Value), String>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok((text, merged)) => self.answer_merge(request_id, &text, Some(merged)),
            Err(reason) => self.answer_merge(request_id, &format!("Refused: {reason}"), None),
        }
        cx.notify();
    }

    fn handle(&mut self, message: PiMessage, cx: &mut Context<Self>) {
        match message {
            PiMessage::Response(response) => {
                // A `!` command can change files, or worktrees, like a turn.
                let shell = response.command == "bash";
                self.handle_response(response);
                if shell {
                    cx.emit(SessionEvent::TurnEnded);
                }
            }
            PiMessage::Event(event) => self.handle_event(&event, cx),
            PiMessage::Exited { code, stderr } => {
                self.client = None;
                let was_running = self.is_running();
                if was_running || code.is_some_and(|c| c != 0) {
                    let detail = stderr
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("");
                    self.exited =
                        Some(format!("Pi exited (code {}). {detail}", code.unwrap_or(-1)));
                    self.transcript
                        .apply(&serde_json::json!({"type": "agent_settled"}));
                }
                self.subagent_requests.clear();
                self.approvals.clear();
                self.merge = None;
                self.transcript.abandon_shells();
                cx.emit(SessionEvent::StatusChanged);
            }
        }
        self.flush_saves();
        self.sync_texts(cx);
        cx.notify();
    }

    /// Sends what the transcript wants saved in Pi's session file, or keeps
    /// it until Pi runs.
    fn flush_saves(&mut self) {
        self.unsaved.extend(self.transcript.take_saves());
        if let Some(client) = &self.client {
            for (kind, data) in self.unsaved.drain(..) {
                client.send(command::save(kind, &data));
            }
        }
    }

    fn handle_response(&mut self, response: crate::pi::rpc::Response) {
        if response.command == "bash" {
            let id = response.id.clone().unwrap_or_default();
            let error = (!response.success).then(|| {
                response
                    .error
                    .clone()
                    .unwrap_or_else(|| "The command failed".into())
            });
            return self.transcript.finish_shell(&id, &response.data, error);
        }
        if !response.success {
            if response.command == "compact" {
                self.transcript.compacting = false;
            }
            if let Some(error) = response.error {
                self.transcript.saved_error(error);
            }
            return;
        }
        let data = &response.data;
        match response.command.as_str() {
            "get_state" => {
                if let Some(file) = data.get("sessionFile").and_then(Value::as_str) {
                    let file = PathBuf::from(file);
                    match &self.kind {
                        SessionKind::Worktree => crate::history::remember(&self.cwd, &file),
                        SessionKind::SubAgent {
                            title,
                            task,
                            tool_call_id,
                        } => crate::history::remember_subagent(
                            &self.cwd,
                            crate::history::SavedSubAgent {
                                tool_call_id: tool_call_id.clone(),
                                file: file.clone(),
                                title: title.to_string(),
                                task: task.clone(),
                            },
                        ),
                    }
                    self.session_file = Some(file);
                }
                if let Some(model) = ModelInfo::from_json(&data["model"]) {
                    self.model = Some(model);
                }
                if let Some(level) = data.get("thinkingLevel").and_then(Value::as_str) {
                    self.thinking = level.to_string();
                }
            }
            "get_available_models" => {
                self.models = data["models"]
                    .as_array()
                    .map(|models| models.iter().filter_map(ModelInfo::from_json).collect())
                    .unwrap_or_default();
            }
            "set_model" => {
                if let Some(model) = ModelInfo::from_json(data) {
                    self.model = Some(model);
                }
            }
            "get_commands" => self.commands = crate::slash::parse_commands(data),
            "new_session" if !data["cancelled"].as_bool().unwrap_or(false) => {
                self.transcript.clear();
                self.texts.clear();
                self.approvals.clear();
                self.context_tokens = None;
                self.started = Instant::now();
                // Saved into the new session, which it opens.
                self.transcript.saved_notice("New session");
                // Pi starts a new file; ask for it so a restart resumes this one.
                if let Some(client) = &self.client {
                    client.send(command::get_state());
                }
            }
            "compact" => self.transcript.compacting = false,
            "get_session_stats" => {
                self.context_tokens = data["contextUsage"]["tokens"].as_u64();
                if let (Some(model), Some(window)) = (
                    self.model.as_mut(),
                    data["contextUsage"]["contextWindow"].as_u64(),
                ) {
                    model.context_window = Some(window);
                }
            }
            _ => {}
        }
    }

    fn handle_event(&mut self, event: &Value, cx: &mut Context<Self>) {
        let before = self.status();
        if event.get("type").and_then(Value::as_str) == Some("extension_ui_request") {
            self.handle_ui_request(event, cx);
        } else {
            match self.transcript.apply(event) {
                Effect::TurnEnded => {
                    if let Some(client) = &self.client {
                        client.send(command::get_session_stats());
                    }
                    cx.emit(SessionEvent::TurnEnded);
                }
                Effect::Settled => {
                    self.viewed = false;
                    self.card_new_commits(None, true);
                    if let Some(client) = &self.client {
                        client.send(command::get_session_stats());
                    }
                }
                Effect::None => {}
            }
            // Commits come from Pi's shell; a card follows the call that made them.
            if event["type"] == "tool_execution_end" && event["toolName"] == "bash" {
                let call_id = str_field(event, "toolCallId");
                self.card_new_commits(Some(&call_id), false);
            }
            if event.get("type").and_then(Value::as_str) == Some("message_update")
                && let Some(tokens) = event["usage"]["totalTokens"].as_u64().filter(|t| *t > 0)
            {
                self.context_tokens = Some(tokens);
            }
        }
        if self.status() != before {
            cx.emit(SessionEvent::StatusChanged);
        }
    }

    /// Adds a card for commits made since HEAD was last seen, read from git
    /// rather than from Pi's reply: right after the tool call that made
    /// them, or at the end once the run settles, which also ends the run's
    /// tracking. A rebase moves HEAD without new commits, so no card.
    fn card_new_commits(&mut self, after: Option<&str>, settled: bool) {
        let Some((seen, started)) = self.head_seen.clone() else {
            return;
        };
        if settled {
            self.head_seen = None;
        }
        let Some(head) = crate::git::head(&self.cwd) else {
            return;
        };
        if head == seen {
            return;
        }
        if !settled {
            self.head_seen = Some((head, started));
        }
        let base = self.base.clone();
        let commits = crate::git::commits_since(&self.cwd, &seen, base.as_deref(), started);
        if commits.is_empty() {
            return;
        }
        let ahead = base
            .as_deref()
            .and_then(|base| crate::git::ahead_behind(&self.cwd, base))
            .map(|(ahead, _)| ahead);
        let uncommitted = crate::git::uncommitted(&self.cwd);
        self.transcript
            .push_commits(after, commits, base, ahead, uncommitted);
    }

    fn handle_ui_request(&mut self, event: &Value, cx: &mut Context<Self>) {
        let request_id = str_field(event, "id");
        let method = str_field(event, "method");
        let title = str_field(event, "title");
        let payload: Value = event
            .get("placeholder")
            .and_then(Value::as_str)
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or(Value::Null);

        match (method.as_str(), title.as_str()) {
            ("input", "treehouse:approval") => {
                let tool_name = str_field(&payload, "toolName");
                let tool_call_id = str_field(&payload, "toolCallId");
                let input = payload["input"].clone();
                let config = Config::global(cx);
                let decision = Policy {
                    mode: config.approval,
                    allowed: &config.allowed,
                    always_ask: &config.always_ask,
                }
                .decide(&ToolCallRequest {
                    tool_name: &tool_name,
                    input: &input,
                    worktree: &self.cwd,
                    project: &self.project,
                });
                match decision {
                    Decision::Allow => self.answer(&request_id, Some("allow")),
                    Decision::Ask => {
                        let pattern = (tool_name == "bash")
                            .then(|| input["command"].as_str().map(propose_pattern))
                            .flatten();
                        if let Some(tool) = self.transcript.tool_mut(&tool_call_id) {
                            tool.status = ToolStatus::AwaitingApproval;
                            tool.args = input.clone();
                        }
                        self.approvals.push(PendingApproval {
                            request_id,
                            tool_call_id,
                            tool_name,
                            input,
                            pattern,
                            asked: Instant::now(),
                        });
                    }
                }
            }
            ("input", "treehouse:subagent") => {
                let tool_call_id = str_field(&payload, "toolCallId");
                self.subagent_requests
                    .insert(request_id.clone(), tool_call_id.clone());
                cx.emit(SessionEvent::SubAgentRequested {
                    request_id,
                    tool_call_id,
                    title: str_field(&payload, "title"),
                    task: str_field(&payload, "task"),
                });
            }
            ("input", "treehouse:worktree") => cx.emit(SessionEvent::WorktreeRequested {
                request_id,
                branch: str_field(&payload, "branch"),
                task: str_field(&payload, "task"),
            }),
            ("input", "treehouse:merge") => cx.emit(SessionEvent::MergeRequested {
                request_id,
                tool_call_id: str_field(&payload, "toolCallId"),
                squash_message: str_field(&payload, "squashMessage"),
            }),
            ("notify", _) => {
                let message = str_field(event, "message");
                let kind = str_field(event, "notifyType");
                if let Some((server, url)) = crate::mcp::parse_sign_in_url(&message) {
                    self.transcript.sign_in(&server).url = Some(url);
                } else if let Some((server, detail)) = crate::mcp::parse_signed_in(&message) {
                    self.transcript
                        .finish_sign_in(Some(&server), SignInState::Done(detail));
                } else if let Some(servers) = crate::mcp::parse_attention(&message) {
                    self.transcript.push_mcp_attention(servers);
                } else if message == "Sign-in cancelled." {
                    self.transcript.finish_sign_in(None, SignInState::Cancelled);
                } else if kind == "error" {
                    // An error while a sign-in waits is that sign-in failing.
                    if !self
                        .transcript
                        .finish_sign_in(None, SignInState::Failed(message.clone()))
                    {
                        self.transcript.saved_error(message);
                    }
                } else {
                    self.transcript.saved_notice(message);
                }
            }
            ("input", title) if crate::mcp::parse_waiting(title).is_some() => {
                let server = crate::mcp::parse_waiting(title).unwrap_or_default();
                self.transcript.sign_in(&server).request_id = Some(request_id);
                cx.emit(SessionEvent::StatusChanged);
            }
            // Other extensions' dialogs have no UI in Treehouse yet.
            ("select" | "confirm" | "input" | "editor", _) => self.answer(&request_id, None),
            _ => {}
        }
    }

    /// Streams new assistant text into its markdown view.
    fn sync_texts(&mut self, cx: &mut Context<Self>) {
        for item in self.transcript.items() {
            let Item::Text {
                id,
                text,
                streaming,
            } = item
            else {
                continue;
            };
            match self.texts.get_mut(id) {
                Some((_, usize::MAX)) => {}
                Some((state, synced)) if !*streaming && *synced == text.len() => {
                    // Right-aligning number columns rewrites the table's
                    // delimiter row, so it waits until the reply is complete.
                    let aligned = crate::markdown::align_numeric_columns(text);
                    if aligned != *text {
                        state.update(cx, |state, cx| state.set_text(&aligned, cx));
                    }
                    *synced = usize::MAX;
                }
                Some((state, synced)) => {
                    if text.len() > *synced
                        && text.starts_with(&text[..*synced])
                        && text.is_char_boundary(*synced)
                    {
                        let delta = text[*synced..].to_string();
                        state.update(cx, |state, cx| state.push_str(&delta, cx));
                        *synced = text.len();
                    } else if text.len() < *synced {
                        let text = text.clone();
                        state.update(cx, |state, cx| state.set_text(&text, cx));
                        *synced = text.len();
                    }
                }
                None => {
                    let source = text.clone();
                    let state = cx.new(|cx| TextViewState::markdown(&source, cx).selectable(true));
                    self.texts.insert(*id, (state, source.len()));
                }
            }
        }
    }
}

pub enum ApprovalChoice {
    Run,
    AlwaysAllow(String),
    Deny(String),
}

/// Links, then runs the script, on a background thread, reporting each
/// step and line of output.
fn run_setup_blocking(
    root: &std::path::Path,
    worktree: &std::path::Path,
    stop: SetupStop,
    tx: smol::channel::Sender<SetupMessage>,
) {
    use std::io::BufRead as _;
    let _ = tx.send_blocking(SetupMessage::Links(crate::setup::link_all(root, worktree)));
    let done = |exit_code, cancelled, error| {
        let _ = tx.send_blocking(SetupMessage::Done {
            exit_code,
            cancelled,
            error,
        });
    };
    let mut command = match crate::setup::prepare_command(root, worktree) {
        None => return done(None, false, None),
        Some(Err(error)) => return done(None, false, Some(error)),
        Some(Ok(command)) => command,
    };
    let spawned = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(err) => return done(None, false, Some(format!("Couldn't run it: {err}"))),
    };
    let stderr = child.stderr.take();
    let stdout = child.stdout.take();
    if let Ok(mut slot) = stop.child.lock() {
        *slot = Some(child);
    }
    let errors = stderr.map(|stderr| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
            {
                let _ = tx.send_blocking(SetupMessage::Output(line));
            }
        })
    });
    if let Some(stdout) = stdout {
        for line in std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
        {
            let _ = tx.send_blocking(SetupMessage::Output(line));
        }
    }
    if let Some(errors) = errors {
        let _ = errors.join();
    }
    let status = stop
        .child
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .and_then(|mut child| child.wait().ok());
    let cancelled = stop.stopped.load(std::sync::atomic::Ordering::SeqCst);
    done(
        status.and_then(|s| s.code()).map(i64::from),
        cancelled,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::AgentStatus;

    #[test]
    fn most_urgent_status_wins() {
        use AgentStatus::*;
        assert_eq!(AgentStatus::most_urgent([Idle, Working, Finished]), Working);
        assert_eq!(
            AgentStatus::most_urgent([Working, NeedsInput, Failed]),
            NeedsInput
        );
        assert_eq!(AgentStatus::most_urgent([Finished, Failed]), Failed);
        assert_eq!(AgentStatus::most_urgent([]), Idle);
    }
}
