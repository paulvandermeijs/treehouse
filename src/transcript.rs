//! The chat history of one Pi session, built from RPC events.
//!
//! This is plain data so it can be tested without a window. The session entity
//! feeds it events and renders its items.

use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::pi::rpc::str_field;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

impl ItemId {
    /// A placeholder until the transcript gives the item its id.
    pub fn none() -> Self {
        ItemId(0)
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Item {
    User {
        id: ItemId,
        text: String,
    },
    Text {
        id: ItemId,
        text: String,
        streaming: bool,
    },
    Thinking {
        id: ItemId,
        text: String,
        streaming: bool,
        started: Instant,
        elapsed: Option<Duration>,
    },
    Tool(ToolCall),
    /// A centred line for events that change the session but aren't messages.
    Notice {
        id: ItemId,
        text: String,
    },
    /// A model or provider error, shown in a red-tinted box.
    Error {
        id: ItemId,
        text: String,
        retrying: bool,
    },
    /// MCP servers Pi couldn't use, with what's wrong with each.
    McpAttention {
        id: ItemId,
        servers: Vec<(String, String)>,
    },
    /// A sign-in to an MCP server.
    SignIn(SignIn),
    /// A command the user ran with `!`.
    Shell(ShellRun),
    /// The commits a turn made, read from git once it settled.
    Commits(CommitCard),
    /// A task another chat handed to this worktree when it asked for it.
    Handover {
        id: ItemId,
        /// The chat it came from, by branch.
        from: String,
        task: String,
    },
}

/// The session entries Treehouse saves for what Pi's own messages don't
/// hold, through the bridge's `/treehouse-save`.
pub const COMMITS_ENTRY: &str = "treehouse:commits";
pub const NOTICE_ENTRY: &str = "treehouse:notice";
pub const ERROR_ENTRY: &str = "treehouse:error";
pub const SETUP_ENTRY: &str = "treehouse:setup";
pub const MCP_ATTENTION_ENTRY: &str = "treehouse:mcp-attention";
pub const SIGN_IN_ENTRY: &str = "treehouse:sign-in";
pub const HANDOVER_ENTRY: &str = "treehouse:handover";

/// A finished setup row, as it's saved.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedSetup {
    command: String,
    output: String,
    exit_code: Option<i64>,
    cancelled: bool,
    error: Option<String>,
    links: Vec<crate::setup::Link>,
    duration_ms: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommitCard {
    #[serde(skip, default = "ItemId::none")]
    pub id: ItemId,
    pub commits: Vec<crate::git::Commit>,
    pub base: Option<String>,
    /// Commits the base lacks, when there is a base.
    pub ahead: Option<usize>,
    pub uncommitted: usize,
}

#[derive(Clone, Debug)]
pub struct ShellRun {
    pub id: ItemId,
    /// The id of the `bash` command, which its updates and result carry.
    pub request_id: String,
    pub command: String,
    /// `!!`: Pi keeps the output out of the model's context.
    pub excluded: bool,
    pub output: String,
    pub running: bool,
    pub exit_code: Option<i64>,
    pub cancelled: bool,
    /// Pi cut the output; the whole of it is in this file.
    pub full_output: Option<String>,
    pub error: Option<String>,
    pub started: Instant,
    pub duration: Option<Duration>,
    /// Set for the worktree's setup (`.agents/linked` and `.agents/prepare`),
    /// with what linking did; `None` for a command you ran.
    pub setup: Option<Vec<crate::setup::Link>>,
}

/// The request id of the worktree's setup run.
pub const SETUP_ID: &str = "setup";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SignInState {
    /// Waiting for the browser.
    Waiting,
    Done(String),
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct SignIn {
    pub id: ItemId,
    pub server: String,
    pub url: Option<String>,
    /// Pi's request for a pasted redirect URL, while it waits.
    pub request_id: Option<String>,
    pub state: SignInState,
}

impl Item {
    pub fn id(&self) -> ItemId {
        match self {
            Item::User { id, .. }
            | Item::Text { id, .. }
            | Item::Thinking { id, .. }
            | Item::Notice { id, .. }
            | Item::Error { id, .. }
            | Item::McpAttention { id, .. } => *id,
            Item::SignIn(sign_in) => sign_in.id,
            Item::Shell(run) => run.id,
            Item::Tool(tool) => tool.id,
            Item::Commits(card) => card.id,
            Item::Handover { id, .. } => *id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    /// The model is still writing the call's arguments.
    Preparing,
    AwaitingApproval,
    Running,
    Done,
    Failed,
    Denied,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: ItemId,
    pub call_id: String,
    pub name: String,
    pub args: Value,
    pub status: ToolStatus,
    /// Text content of the (partial) result.
    pub output: String,
    pub details: Value,
    pub started: Instant,
    pub duration: Option<Duration>,
    /// For `write`: what the file held before Pi wrote it, if it existed.
    pub previous_content: Option<String>,
}

impl ToolCall {
    /// Pi rejected the call because it has no tool by that name, often a
    /// model writing its own markup into the name.
    pub fn is_unknown_tool(&self) -> bool {
        self.status == ToolStatus::Failed
            && self.output.trim() == format!("Tool {} not found", self.name)
    }

    /// The tool name up to any leaked model markup, on one line.
    pub fn clean_name(&self) -> &str {
        let name = self.name.split("<|").next().unwrap_or("");
        name.lines().next().unwrap_or("").trim()
    }

    pub fn path(&self) -> Option<&str> {
        self.args.get("path").and_then(Value::as_str)
    }

    pub fn command(&self) -> Option<&str> {
        self.args.get("command").and_then(Value::as_str)
    }

    /// The exit code of a bash call, parsed from Pi's result text.
    pub fn exit_code(&self) -> Option<i32> {
        if self.name != "bash" {
            return None;
        }
        match self.status {
            ToolStatus::Done => Some(0),
            ToolStatus::Failed => self
                .output
                .lines()
                .rev()
                .find_map(|line| line.trim().strip_prefix("Command exited with code "))
                .and_then(|code| code.trim().parse().ok()),
            _ => None,
        }
    }

    /// The unified patch of an edit, or of a write that replaced a file.
    pub fn patch(&self) -> Option<String> {
        match self.name.as_str() {
            "edit" => self
                .details
                .get("patch")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "write" => {
                let path = self.path()?;
                let new = self.args.get("content").and_then(Value::as_str)?;
                Some(crate::git::unified_patch(
                    path,
                    self.previous_content.as_deref(),
                    new,
                ))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    Failed,
}

/// What the session should do after an event, beyond updating items.
#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    /// A turn ended: refresh token counts and the diff.
    TurnEnded,
    /// Nothing more will happen until the user acts.
    Settled,
}

pub struct Transcript {
    items: Vec<Item>,
    next_id: u64,
    /// Maps the streaming assistant message's content indexes to items.
    blocks: Vec<(u64, ItemId)>,
    pub run: RunState,
    /// When the current or last run started and when it settled, for the
    /// sidebar's status card.
    pub run_started: Option<Instant>,
    pub settled_at: Option<Instant>,
    /// Set while Pi compacts the context.
    pub compacting: bool,
    /// Entries to save in Pi's session file, for items Pi's own messages
    /// don't hold; the session sends them once Pi runs.
    saves: Vec<(&'static str, Value)>,
    /// A handed-over task whose message Pi hasn't echoed yet: the card shows
    /// it, so the message doesn't.
    handed_over: Option<String>,
    /// What the user typed for messages Pi hasn't echoed yet. Pi echoes a
    /// skill or template command expanded, so the chat shows this instead.
    typed: VecDeque<String>,
    cwd: PathBuf,
}

impl Transcript {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
            blocks: Vec::new(),
            run: RunState::Idle,
            run_started: None,
            settled_at: None,
            compacting: false,
            saves: Vec::new(),
            handed_over: None,
            typed: VecDeque::new(),
            cwd: cwd.into(),
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn tool(&self, call_id: &str) -> Option<&ToolCall> {
        self.items.iter().find_map(|item| match item {
            Item::Tool(tool) if tool.call_id == call_id => Some(tool),
            _ => None,
        })
    }

    pub fn tool_mut(&mut self, call_id: &str) -> Option<&mut ToolCall> {
        self.items.iter_mut().find_map(|item| match item {
            Item::Tool(tool) if tool.call_id == call_id => Some(tool),
            _ => None,
        })
    }

    /// Records a message as the user typed it, for when Pi echoes it.
    pub fn expect_user_text(&mut self, text: impl Into<String>) {
        self.typed.push_back(text.into());
    }

    /// Empties the chat for a new session. Item ids keep counting, so views
    /// keyed by them never mix up old and new items.
    pub fn clear(&mut self) {
        self.items.clear();
        self.blocks.clear();
        self.typed.clear();
        self.run = RunState::Idle;
        self.compacting = false;
    }

    /// An item Treehouse saved with `/treehouse-save`, back from the
    /// session file.
    fn restore_saved(&mut self, kind: Option<&str>, data: &Value) {
        let text = || str_field(data, "text");
        match kind {
            Some(COMMITS_ENTRY) => {
                if let Ok(mut card) = serde_json::from_value::<CommitCard>(data.clone()) {
                    card.id = self.alloc();
                    self.items.push(Item::Commits(card));
                }
            }
            Some(NOTICE_ENTRY) => self.push_notice(text()),
            Some(ERROR_ENTRY) => self.push_error(text()),
            Some(SETUP_ENTRY) => {
                let Ok(saved) = serde_json::from_value::<SavedSetup>(data.clone()) else {
                    return;
                };
                let id = self.alloc();
                self.items.push(Item::Shell(ShellRun {
                    id,
                    request_id: SETUP_ID.to_string(),
                    command: saved.command,
                    excluded: true,
                    output: saved.output,
                    running: false,
                    exit_code: saved.exit_code,
                    cancelled: saved.cancelled,
                    full_output: None,
                    error: saved.error,
                    started: Instant::now(),
                    duration: Some(Duration::from_millis(saved.duration_ms)),
                    setup: Some(saved.links),
                }));
            }
            Some(HANDOVER_ENTRY) => {
                let task = str_field(data, "task");
                let id = self.alloc();
                self.items.push(Item::Handover {
                    id,
                    from: str_field(data, "from"),
                    task: task.clone(),
                });
                self.handed_over = Some(task);
            }
            Some(MCP_ATTENTION_ENTRY) => {
                if let Ok(servers) = serde_json::from_value(data["servers"].clone()) {
                    let id = self.alloc();
                    self.items.push(Item::McpAttention { id, servers });
                }
            }
            Some(SIGN_IN_ENTRY) => {
                if let Ok(state) = serde_json::from_value(data["state"].clone()) {
                    let id = self.alloc();
                    self.items.push(Item::SignIn(SignIn {
                        id,
                        server: str_field(data, "server"),
                        url: data["url"].as_str().map(str::to_string),
                        request_id: None,
                        state,
                    }));
                }
            }
            _ => {}
        }
    }

    /// Rebuilds the chat from the messages of a saved Pi session, as
    /// `crate::history::load` returns them. Pi's events never replay, so the
    /// items are built from the stored messages themselves.
    pub fn restore(&mut self, entries: &[Value]) {
        // When each tool call was made, for its duration once the result is in.
        let mut called_at: std::collections::HashMap<String, u64> = Default::default();
        for entry in entries {
            match entry.get("type").and_then(Value::as_str) {
                Some("compaction") => {
                    let before = entry.get("tokensBefore").and_then(Value::as_u64);
                    self.push_notice(match before {
                        Some(before) => {
                            format!(
                                "Context compacted · {} tokens before",
                                format_tokens(before)
                            )
                        }
                        None => "Context compacted".to_string(),
                    });
                    continue;
                }
                Some("custom") => {
                    self.restore_saved(entry["customType"].as_str(), &entry["data"]);
                    continue;
                }
                // Pi records the model and thinking level a session starts
                // with too; only changes during the chat get a notice.
                Some("model_change") if !self.items.is_empty() => {
                    self.push_notice(format!("Model changed to {}", str_field(entry, "modelId")));
                    continue;
                }
                Some("thinking_level_change") if !self.items.is_empty() => {
                    self.push_notice(format!(
                        "Thinking level changed to {}",
                        str_field(entry, "thinkingLevel")
                    ));
                    continue;
                }
                Some("message") => {}
                _ => continue,
            }
            let message = &entry["message"];
            let timestamp = message.get("timestamp").and_then(Value::as_u64);
            match message.get("role").and_then(Value::as_str) {
                Some("user") => {
                    let id = self.alloc();
                    let text = user_text(&message["content"]);
                    let text = skill_command(&text).unwrap_or(text);
                    if self.handed_over.as_ref() == Some(&text) {
                        self.handed_over = None;
                        continue;
                    }
                    self.items.push(Item::User { id, text });
                }
                Some("assistant") => {
                    for block in message["content"].as_array().into_iter().flatten() {
                        match block.get("type").and_then(Value::as_str) {
                            Some("text") => {
                                let text = str_field(block, "text");
                                if text.trim().is_empty() {
                                    continue;
                                }
                                let id = self.alloc();
                                self.items.push(Item::Text {
                                    id,
                                    text,
                                    streaming: false,
                                });
                            }
                            Some("thinking") => {
                                let text = str_field(block, "thinking");
                                if text.trim().is_empty() {
                                    continue;
                                }
                                let id = self.alloc();
                                self.items.push(Item::Thinking {
                                    id,
                                    text,
                                    streaming: false,
                                    started: Instant::now(),
                                    elapsed: None,
                                });
                            }
                            Some("toolCall") => {
                                let call_id = str_field(block, "id");
                                let name = str_field(block, "name");
                                if let Some(at) = timestamp {
                                    called_at.insert(call_id.clone(), at);
                                }
                                let tool = self.ensure_tool(&call_id, &name);
                                tool.args = block["arguments"].clone();
                                // Without a result Pi was stopped mid-call.
                                tool.status = ToolStatus::Stopped;
                            }
                            _ => {}
                        }
                    }
                    match message.get("stopReason").and_then(Value::as_str) {
                        Some("error") => {
                            let text = message
                                .get("errorMessage")
                                .and_then(Value::as_str)
                                .unwrap_or("The request failed.");
                            self.push_error(format!("Request failed: {text}"));
                        }
                        Some("aborted") => self.push_notice("Stopped by you"),
                        _ => {}
                    }
                }
                Some("toolResult") => {
                    let call_id = str_field(message, "toolCallId");
                    let started = called_at.get(&call_id).copied();
                    let Some(tool) = self.tool_mut(&call_id) else {
                        continue;
                    };
                    tool.output = content_text(&message["content"]);
                    tool.details = message.get("details").cloned().unwrap_or(Value::Null);
                    tool.duration = started
                        .zip(timestamp)
                        .map(|(start, end)| Duration::from_millis(end.saturating_sub(start)));
                    let is_error = message
                        .get("isError")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    tool.status = if !is_error {
                        ToolStatus::Done
                    } else if tool.output.contains("Command aborted") {
                        ToolStatus::Stopped
                    } else if tool.output.contains("Denied by the user")
                        || tool.output.contains("Stopped by the user")
                    {
                        ToolStatus::Denied
                    } else {
                        ToolStatus::Failed
                    };
                }
                Some("bashExecution") => {
                    let id = self.alloc();
                    self.items.push(Item::Shell(ShellRun {
                        id,
                        request_id: String::new(),
                        command: str_field(message, "command"),
                        excluded: message
                            .get("excludeFromContext")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        output: str_field(message, "output"),
                        running: false,
                        exit_code: message.get("exitCode").and_then(Value::as_i64),
                        cancelled: message
                            .get("cancelled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        full_output: message
                            .get("fullOutputPath")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        error: None,
                        started: Instant::now(),
                        duration: None,
                        setup: None,
                    }));
                }
                _ => {}
            }
        }
    }

    pub fn push_mcp_attention(&mut self, servers: Vec<(String, String)>) {
        self.saves.push((
            MCP_ATTENTION_ENTRY,
            serde_json::json!({ "servers": servers }),
        ));
        let id = self.alloc();
        self.items.push(Item::McpAttention { id, servers });
    }

    /// The sign-in to `server` that is still waiting, if any.
    pub fn waiting_sign_in(&mut self, server: Option<&str>) -> Option<&mut SignIn> {
        self.items.iter_mut().rev().find_map(|item| match item {
            Item::SignIn(sign_in)
                if sign_in.state == SignInState::Waiting
                    && server.is_none_or(|s| s == sign_in.server) =>
            {
                Some(sign_in)
            }
            _ => None,
        })
    }

    /// Starts a sign-in again in place when the last item is that server's
    /// cancelled or failed one, instead of stacking a new card under it.
    pub fn retry_sign_in(&mut self, server: &str) {
        if let Some(Item::SignIn(sign_in)) = self.items.last_mut()
            && sign_in.server == server
            && matches!(
                sign_in.state,
                SignInState::Cancelled | SignInState::Failed(_)
            )
        {
            sign_in.state = SignInState::Waiting;
            sign_in.url = None;
        }
    }

    /// A sign-in started or got its browser URL or pending request.
    pub fn sign_in(&mut self, server: &str) -> &mut SignIn {
        if self.waiting_sign_in(Some(server)).is_none() {
            let id = self.alloc();
            self.items.push(Item::SignIn(SignIn {
                id,
                server: server.to_string(),
                url: None,
                request_id: None,
                state: SignInState::Waiting,
            }));
        }
        self.waiting_sign_in(Some(server)).unwrap()
    }

    pub fn finish_sign_in(&mut self, server: Option<&str>, state: SignInState) -> bool {
        match self.waiting_sign_in(server) {
            Some(sign_in) => {
                sign_in.state = state;
                sign_in.request_id = None;
                let saved = serde_json::json!({
                    "server": sign_in.server,
                    "url": sign_in.url,
                    "state": sign_in.state,
                });
                self.saves.push((SIGN_IN_ENTRY, saved));
                true
            }
            None => false,
        }
    }

    /// Adds a commit card right after the tool call that made the commits,
    /// or at the end without one.
    pub fn push_commits(
        &mut self,
        after: Option<&str>,
        commits: Vec<crate::git::Commit>,
        base: Option<String>,
        ahead: Option<usize>,
        uncommitted: usize,
    ) {
        let card = CommitCard {
            id: self.alloc(),
            commits,
            base,
            ahead,
            uncommitted,
        };
        if let Ok(saved) = serde_json::to_value(&card) {
            self.saves.push((COMMITS_ENTRY, saved));
        }
        let at = after.and_then(|call_id| {
            self.items
                .iter()
                .position(|item| matches!(item, Item::Tool(tool) if tool.call_id == call_id))
        });
        match at {
            Some(ix) => self.items.insert(ix + 1, Item::Commits(card)),
            None => self.items.push(Item::Commits(card)),
        }
    }

    /// Shows a task handed over by another chat, saved with the chat; the
    /// message that sends it to Pi then doesn't show again.
    pub fn hand_over(&mut self, from: String, task: String) {
        self.saves.push((
            HANDOVER_ENTRY,
            serde_json::json!({ "from": from, "task": task }),
        ));
        // First, above the setup row that started with the worktree, as the
        // saved entry restores it.
        let id = self.alloc();
        self.items.insert(
            0,
            Item::Handover {
                id,
                from,
                task: task.clone(),
            },
        );
        self.handed_over = Some(task);
    }

    /// The entries to save since this was last called.
    pub fn take_saves(&mut self) -> Vec<(&'static str, Value)> {
        std::mem::take(&mut self.saves)
    }

    /// A notice Pi's history doesn't hold, saved so a restart shows it.
    pub fn saved_notice(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.saves
            .push((NOTICE_ENTRY, serde_json::json!({ "text": text })));
        self.push_notice(text);
    }

    /// An error Pi's history doesn't hold, saved so a restart shows it.
    pub fn saved_error(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.saves
            .push((ERROR_ENTRY, serde_json::json!({ "text": text })));
        self.push_error(text);
    }

    pub fn push_notice(&mut self, text: impl Into<String>) {
        let id = self.alloc();
        self.items.push(Item::Notice {
            id,
            text: text.into(),
        });
    }

    pub fn push_error(&mut self, text: impl Into<String>) {
        let id = self.alloc();
        self.items.push(Item::Error {
            id,
            text: text.into(),
            retrying: false,
        });
    }

    pub fn start_shell(&mut self, request_id: String, command: &str, excluded: bool) {
        let id = self.alloc();
        self.items.push(Item::Shell(ShellRun {
            id,
            request_id,
            command: command.to_string(),
            excluded,
            output: String::new(),
            running: true,
            exit_code: None,
            cancelled: false,
            full_output: None,
            error: None,
            started: Instant::now(),
            duration: None,
            setup: None,
        }));
    }

    /// The worktree's setup starts: a command row Treehouse runs itself.
    pub fn start_setup(&mut self, title: &str) {
        let id = self.alloc();
        self.items.push(Item::Shell(ShellRun {
            id,
            request_id: SETUP_ID.to_string(),
            command: title.to_string(),
            excluded: true,
            output: String::new(),
            running: true,
            exit_code: None,
            cancelled: false,
            full_output: None,
            error: None,
            started: Instant::now(),
            duration: None,
            setup: Some(Vec::new()),
        }));
    }

    pub fn setup_run(&mut self) -> Option<&mut ShellRun> {
        self.shell_mut(SETUP_ID)
    }

    pub fn setup_running(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, Item::Shell(run) if run.running && run.setup.is_some()))
    }

    /// The setup ended: with the script's exit code, stopped, or an error
    /// that kept it from running.
    pub fn finish_setup(&mut self, exit_code: Option<i64>, cancelled: bool, error: Option<String>) {
        let Some(run) = self.setup_run() else {
            return;
        };
        run.running = false;
        run.duration = Some(run.started.elapsed());
        run.exit_code = exit_code;
        run.cancelled = cancelled;
        run.error = error;
        let saved = SavedSetup {
            command: run.command.clone(),
            output: run.output.clone(),
            exit_code: run.exit_code,
            cancelled: run.cancelled,
            error: run.error.clone(),
            links: run.setup.clone().unwrap_or_default(),
            duration_ms: run.duration.map(|d| d.as_millis() as u64).unwrap_or(0),
        };
        if let Ok(saved) = serde_json::to_value(saved) {
            self.saves.push((SETUP_ENTRY, saved));
        }
    }

    pub fn shell_mut(&mut self, request_id: &str) -> Option<&mut ShellRun> {
        self.items.iter_mut().rev().find_map(|item| match item {
            Item::Shell(run) if run.request_id == request_id => Some(run),
            _ => None,
        })
    }

    pub fn shell_running(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, Item::Shell(run) if run.running))
    }

    /// Ends `!` commands that will never get a result because Pi stopped.
    pub fn abandon_shells(&mut self) {
        for item in self.items.iter_mut() {
            // The setup runs without Pi, so it carries on.
            if let Item::Shell(run) = item
                && run.running
                && run.setup.is_none()
            {
                run.running = false;
                run.duration = Some(run.started.elapsed());
                run.error = Some("Pi stopped before the command finished".into());
            }
        }
    }

    /// Applies a `bash` response: Pi's `BashResult`, or the error it failed with.
    pub fn finish_shell(&mut self, request_id: &str, result: &Value, error: Option<String>) {
        let Some(run) = self.shell_mut(request_id) else {
            return;
        };
        run.running = false;
        run.duration = Some(run.started.elapsed());
        if let Some(output) = result.get("output").and_then(Value::as_str) {
            run.output = output.to_string();
        }
        run.exit_code = result.get("exitCode").and_then(Value::as_i64);
        run.cancelled = result
            .get("cancelled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if result.get("truncated").and_then(Value::as_bool) == Some(true) {
            run.full_output = result
                .get("fullOutputPath")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        run.error = error;
    }

    fn alloc(&mut self) -> ItemId {
        let id = ItemId(self.next_id);
        self.next_id += 1;
        id
    }

    fn block(&self, index: u64) -> Option<ItemId> {
        self.blocks
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, id)| *id)
    }

    fn item_mut(&mut self, id: ItemId) -> Option<&mut Item> {
        self.items.iter_mut().rev().find(|item| item.id() == id)
    }

    fn ensure_tool(&mut self, call_id: &str, name: &str) -> &mut ToolCall {
        if self.tool(call_id).is_none() {
            let id = self.alloc();
            self.items.push(Item::Tool(ToolCall {
                id,
                call_id: call_id.to_string(),
                name: name.to_string(),
                args: Value::Null,
                status: ToolStatus::Preparing,
                output: String::new(),
                details: Value::Null,
                started: Instant::now(),
                duration: None,
                previous_content: None,
            }));
        }
        self.tool_mut(call_id).unwrap()
    }

    pub fn apply(&mut self, event: &Value) -> Effect {
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "agent_start" => {
                // Pi starts a run again after a retry or a steer; the turn
                // started with the first.
                if self.run != RunState::Running {
                    self.run_started = Some(Instant::now());
                }
                self.run = RunState::Running;
                Effect::None
            }
            "message_start" => {
                let message = &event["message"];
                match message.get("role").and_then(Value::as_str) {
                    Some("user") => {
                        let text = self
                            .typed
                            .pop_front()
                            .unwrap_or_else(|| user_text(&message["content"]));
                        if self.handed_over.as_ref() == Some(&text) {
                            self.handed_over = None;
                        } else {
                            let id = self.alloc();
                            self.items.push(Item::User { id, text });
                        }
                    }
                    Some("assistant") => self.blocks.clear(),
                    _ => {}
                }
                Effect::None
            }
            "message_update" => {
                self.apply_assistant_event(&event["assistantMessageEvent"]);
                Effect::None
            }
            "message_end" => {
                let message = &event["message"];
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    self.finish_streaming_blocks();
                    match message.get("stopReason").and_then(Value::as_str) {
                        Some("error") => {
                            let text = message
                                .get("errorMessage")
                                .and_then(Value::as_str)
                                .unwrap_or("The request failed.");
                            self.push_error(format!("Request failed: {text}"));
                        }
                        Some("aborted") => self.push_notice("Stopped by you"),
                        _ => {}
                    }
                }
                Effect::None
            }
            "tool_execution_start" => {
                let call_id = str_field(event, "toolCallId");
                let name = str_field(event, "toolName");
                let cwd = self.cwd.clone();
                let tool = self.ensure_tool(&call_id, &name);
                tool.args = event["args"].clone();
                if tool.status != ToolStatus::AwaitingApproval {
                    tool.status = ToolStatus::Running;
                }
                tool.started = Instant::now();
                if tool.name == "write" {
                    tool.previous_content = tool
                        .path()
                        .and_then(|path| std::fs::read_to_string(resolve(&cwd, path)).ok());
                }
                Effect::None
            }
            "tool_execution_update" => {
                let call_id = str_field(event, "toolCallId");
                if let Some(tool) = self.tool_mut(&call_id) {
                    tool.output = content_text(&event["partialResult"]["content"]);
                }
                Effect::None
            }
            "tool_execution_end" => {
                let call_id = str_field(event, "toolCallId");
                let name = str_field(event, "toolName");
                let tool = self.ensure_tool(&call_id, &name);
                let result = &event["result"];
                tool.output = content_text(&result["content"]);
                tool.details = result.get("details").cloned().unwrap_or(Value::Null);
                tool.duration = event
                    .get("durationMs")
                    .and_then(Value::as_u64)
                    .map(Duration::from_millis)
                    .or_else(|| Some(tool.started.elapsed()));
                let is_error = event
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                tool.status = match (is_error, tool.status) {
                    (true, ToolStatus::Denied) => ToolStatus::Denied,
                    (true, _) if tool.output.contains("Command aborted") => ToolStatus::Stopped,
                    (true, _) => ToolStatus::Failed,
                    (false, _) => ToolStatus::Done,
                };
                Effect::None
            }
            "turn_end" => Effect::TurnEnded,
            "auto_retry_start" => {
                let attempt = event.get("attempt").and_then(Value::as_u64).unwrap_or(1);
                let delay = event.get("delayMs").and_then(Value::as_u64).unwrap_or(0) / 1000;
                // Pi's message often ends in a full stop already.
                let error = str_field(event, "errorMessage");
                let error = error.trim_end().trim_end_matches('.');
                let id = self.alloc();
                self.items.push(Item::Error {
                    id,
                    text: format!(
                        "Request failed: {error}. Retrying in {delay}s (attempt {attempt})."
                    ),
                    retrying: true,
                });
                Effect::None
            }
            "auto_retry_end" => {
                for item in self.items.iter_mut() {
                    if let Item::Error { retrying, .. } = item {
                        *retrying = false;
                    }
                }
                Effect::None
            }
            "bash_execution_update" => {
                let request_id = event.get("id").and_then(Value::as_str).unwrap_or_default();
                let delta = event
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(run) = self.shell_mut(request_id) {
                    run.output.push_str(delta);
                }
                Effect::None
            }
            "compaction_start" => {
                self.compacting = true;
                Effect::None
            }
            "compaction_end" => {
                self.compacting = false;
                let result = &event["result"];
                if let Some(before) = result.get("tokensBefore").and_then(Value::as_u64) {
                    let after = result.get("estimatedTokensAfter").and_then(Value::as_u64);
                    let text = match after {
                        Some(after) => format!(
                            "Context compacted · {} → {} tokens",
                            format_tokens(before),
                            format_tokens(after)
                        ),
                        None => format!(
                            "Context compacted · {} tokens before",
                            format_tokens(before)
                        ),
                    };
                    self.push_notice(text);
                } else if let Some(error) = event.get("errorMessage").and_then(Value::as_str) {
                    self.saved_error(format!("Compaction failed: {error}"));
                }
                Effect::None
            }
            "thinking_level_changed" => {
                let level = str_field(event, "level");
                self.push_notice(format!("Thinking level changed to {level}"));
                Effect::None
            }
            "agent_end" => {
                // Pi 1.x follows with `agent_settled`; older versions end here.
                if event.get("willRetry").is_none() {
                    return self.settle();
                }
                Effect::None
            }
            "agent_settled" => self.settle(),
            _ => Effect::None,
        }
    }

    fn settle(&mut self) -> Effect {
        self.finish_streaming_blocks();
        for item in self.items.iter_mut() {
            if let Item::Tool(tool) = item
                && matches!(
                    tool.status,
                    ToolStatus::Running | ToolStatus::Preparing | ToolStatus::AwaitingApproval
                )
            {
                tool.status = ToolStatus::Stopped;
            }
        }
        self.settled_at = Some(Instant::now());
        let failed = matches!(self.items.last(), Some(Item::Error { .. }));
        self.run = if failed {
            RunState::Failed
        } else {
            RunState::Idle
        };
        Effect::Settled
    }

    fn finish_streaming_blocks(&mut self) {
        for item in self.items.iter_mut() {
            match item {
                Item::Text { streaming, .. } => *streaming = false,
                Item::Thinking {
                    streaming,
                    started,
                    elapsed,
                    ..
                } if *streaming => {
                    *streaming = false;
                    *elapsed = Some(started.elapsed());
                }
                _ => {}
            }
        }
    }

    fn apply_assistant_event(&mut self, event: &Value) {
        let index = event
            .get("contentIndex")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text_start" => {
                let id = self.alloc();
                self.items.push(Item::Text {
                    id,
                    text: String::new(),
                    streaming: true,
                });
                self.blocks.push((index, id));
            }
            "text_delta" => {
                let delta = str_field(event, "delta");
                let id = match self.block(index) {
                    Some(id) => id,
                    None => {
                        self.apply_assistant_event(
                            &serde_json::json!({"type": "text_start", "contentIndex": index}),
                        );
                        self.block(index).unwrap()
                    }
                };
                if let Some(Item::Text { text, .. }) = self.item_mut(id) {
                    text.push_str(&delta);
                }
            }
            "text_end" => {
                if let Some(id) = self.block(index) {
                    let content = event
                        .get("content")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    if let Some(Item::Text {
                        text, streaming, ..
                    }) = self.item_mut(id)
                    {
                        if let Some(content) = content {
                            *text = content;
                        }
                        *streaming = false;
                    }
                }
            }
            "thinking_start" => {
                let id = self.alloc();
                self.items.push(Item::Thinking {
                    id,
                    text: String::new(),
                    streaming: true,
                    started: Instant::now(),
                    elapsed: None,
                });
                self.blocks.push((index, id));
            }
            "thinking_delta" => {
                let delta = str_field(event, "delta");
                if let Some(id) = self.block(index)
                    && let Some(Item::Thinking { text, .. }) = self.item_mut(id)
                {
                    text.push_str(&delta);
                }
            }
            "thinking_end" => {
                if let Some(id) = self.block(index)
                    && let Some(Item::Thinking {
                        streaming,
                        started,
                        elapsed,
                        ..
                    }) = self.item_mut(id)
                {
                    *streaming = false;
                    *elapsed = Some(started.elapsed());
                }
            }
            "toolcall_start" => {
                let call_id = str_field(event, "id");
                let name = str_field(event, "toolName");
                if !call_id.is_empty() {
                    self.ensure_tool(&call_id, &name);
                }
            }
            "toolcall_end" => {
                let call = &event["toolCall"];
                let call_id = str_field(call, "id");
                let name = str_field(call, "name");
                if !call_id.is_empty() {
                    let tool = self.ensure_tool(&call_id, &name);
                    tool.args = call["arguments"].clone();
                }
            }
            _ => {}
        }
    }
}

fn resolve(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

/// The text of a user message's content, which is a string or blocks.
/// The command behind a message Pi expanded from a skill, as the user typed
/// it: `/skill:name …`, or `/commit …` for Treehouse's built-in skills.
fn skill_command(text: &str) -> Option<String> {
    let rest = text.strip_prefix("<skill name=\"")?;
    let (name, rest) = rest.split_once('"')?;
    let (_, after) = rest.split_once("\n</skill>")?;
    let command = match name.strip_prefix("treehouse-") {
        Some(lifecycle) => format!("/{lifecycle}"),
        None => format!("/skill:{name}"),
    };
    Some(match after.strip_prefix("\n\n").map(str::trim) {
        Some(args) if !args.is_empty() => format!("{command} {args}"),
        _ => command,
    })
}

fn user_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        _ => content_text(content),
    }
}

/// Joins the text blocks of a content array.
pub fn content_text(content: &Value) -> String {
    content
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// `48200` → `48.2k`.
pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1000 {
        let k = tokens as f64 / 1000.0;
        if k >= 100.0 {
            format!("{k:.0}k")
        } else {
            format!("{k:.1}k")
        }
    } else {
        tokens.to_string()
    }
}

/// `102 s` → `1m 42s`.
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if duration.as_millis() < 10_000 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feed(transcript: &mut Transcript, events: &[Value]) -> Vec<Effect> {
        events.iter().map(|event| transcript.apply(event)).collect()
    }

    #[test]
    fn retry_notice_has_one_full_stop() {
        let mut t = Transcript::new("/tmp");
        feed(
            &mut t,
            &[
                json!({"type": "auto_retry_start", "attempt": 1, "delayMs": 2000, "errorMessage": "Connection error."}),
            ],
        );
        assert!(matches!(
            t.items().last(),
            Some(Item::Error { text, retrying: true, .. })
                if text == "Request failed: Connection error. Retrying in 2s (attempt 1)."
        ));
    }

    #[test]
    fn restores_a_saved_session() {
        let entry = |message: Value| json!({"type": "message", "message": message});
        let entries = [
            entry(json!({"role": "system", "content": ""})),
            entry(json!({"role": "user", "content": [{"type": "text", "text": "Fix the test"}]})),
            entry(json!({"role": "assistant", "timestamp": 1000, "content": [
                {"type": "thinking", "thinking": "Look first."},
                {"type": "text", "text": "Running it."},
                {"type": "toolCall", "id": "c1", "name": "bash", "arguments": {"command": "cargo test"}},
                {"type": "toolCall", "id": "c2", "name": "write", "arguments": {"path": "x"}},
            ]})),
            entry(
                json!({"role": "toolResult", "toolCallId": "c1", "timestamp": 3500, "isError": false, "content": [{"type": "text", "text": "ok"}]}),
            ),
            entry(
                json!({"role": "toolResult", "toolCallId": "c2", "isError": true, "content": [{"type": "text", "text": "Denied by the user in Treehouse"}]}),
            ),
            json!({"type": "compaction", "tokensBefore": 52000}),
            entry(
                json!({"role": "bashExecution", "command": "ls", "output": "src\n", "exitCode": 0, "cancelled": false, "excludeFromContext": true}),
            ),
            entry(
                json!({"role": "assistant", "stopReason": "aborted", "content": [
                    {"type": "toolCall", "id": "c3", "name": "read", "arguments": {"path": "y"}},
                ]}),
            ),
        ];
        let mut t = Transcript::new("/tmp");
        t.restore(&entries);
        let kinds: Vec<&str> = t
            .items
            .iter()
            .map(|item| match item {
                Item::User { .. } => "user",
                Item::Thinking { .. } => "thinking",
                Item::Text { .. } => "text",
                Item::Tool(_) => "tool",
                Item::Notice { .. } => "notice",
                Item::Shell(_) => "shell",
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "user", "thinking", "text", "tool", "tool", "notice", "shell", "tool", "notice"
            ]
        );
        let bash = t.tool("c1").unwrap();
        assert_eq!(bash.status, ToolStatus::Done);
        assert_eq!(bash.output, "ok");
        assert_eq!(bash.duration, Some(Duration::from_millis(2500)));
        assert_eq!(t.tool("c2").unwrap().status, ToolStatus::Denied);
        // No result: Pi stopped before the call ran.
        assert_eq!(t.tool("c3").unwrap().status, ToolStatus::Stopped);
        let Some(Item::Shell(run)) = t.items.get(6) else {
            panic!("no shell row");
        };
        assert!(run.excluded && !run.running && run.exit_code == Some(0));
        assert_eq!(t.run, RunState::Idle);
    }

    #[test]
    fn runs_a_shell_command() {
        let mut t = Transcript::new("/tmp");
        t.start_shell("th-3".into(), "cargo test", false);
        assert!(t.shell_running());
        feed(
            &mut t,
            &[
                json!({"type": "bash_execution_update", "id": "th-3", "delta": "Compiling\n"}),
                json!({"type": "bash_execution_update", "id": "other", "delta": "elsewhere\n"}),
            ],
        );
        t.finish_shell(
            "th-3",
            &json!({"output": "Compiling\nok\n", "exitCode": 0, "cancelled": false, "truncated": true, "fullOutputPath": "/tmp/out"}),
            None,
        );
        let Some(Item::Shell(run)) = t.items.last() else {
            panic!("no shell row");
        };
        assert!(!run.running && !t.shell_running());
        assert_eq!(run.output, "Compiling\nok\n");
        assert_eq!(run.exit_code, Some(0));
        assert_eq!(run.full_output.as_deref(), Some("/tmp/out"));
    }

    #[test]
    fn spots_a_garbled_tool_name() {
        let name = "exec:echo hi > LICENSE\n<|channel>thought\nNow verify.<channel|>";
        let mut t = Transcript::new("/tmp");
        feed(
            &mut t,
            &[
                json!({"type": "message_start", "message": {"role": "assistant", "content": []}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "toolcall_end", "contentIndex": 0, "toolCall": {"id": "c1", "name": name, "arguments": {}}}}),
                json!({"type": "tool_execution_start", "toolCallId": "c1", "toolName": name, "args": {}}),
                json!({"type": "tool_execution_end", "toolCallId": "c1", "toolName": name, "result": {"content": [{"type": "text", "text": format!("Tool {name} not found")}]}, "isError": true}),
            ],
        );
        let Some(Item::Tool(tool)) = t.items.last() else {
            panic!("no tool row");
        };
        assert!(tool.is_unknown_tool());
        assert_eq!(tool.clean_name(), "exec:echo hi > LICENSE");
    }

    #[test]
    fn builds_a_turn_from_events() {
        let mut t = Transcript::new("/tmp");
        let effects = feed(
            &mut t,
            &[
                json!({"type": "agent_start"}),
                json!({"type": "message_start", "message": {"role": "user", "content": "Hi"}}),
                json!({"type": "message_start", "message": {"role": "assistant", "content": []}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "thinking_start", "contentIndex": 0}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": "Hmm"}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "thinking_end", "contentIndex": 0}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "text_start", "contentIndex": 1}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": "Hel"}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": "lo"}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "toolcall_start", "contentIndex": 2, "id": "c1", "toolName": "read"}}),
                json!({"type": "message_update", "assistantMessageEvent": {"type": "toolcall_end", "contentIndex": 2, "toolCall": {"id": "c1", "name": "read", "arguments": {"path": "src/main.rs"}}}}),
                json!({"type": "message_end", "message": {"role": "assistant", "stopReason": "toolUse"}}),
                json!({"type": "tool_execution_start", "toolCallId": "c1", "toolName": "read", "args": {"path": "src/main.rs"}}),
                json!({"type": "tool_execution_end", "toolCallId": "c1", "toolName": "read", "result": {"content": [{"type": "text", "text": "fn main() {}"}]}, "isError": false, "durationMs": 12}),
                json!({"type": "turn_end"}),
                json!({"type": "agent_end", "willRetry": false}),
                json!({"type": "agent_settled", "aborted": false}),
            ],
        );
        assert_eq!(effects.iter().filter(|e| **e == Effect::Settled).count(), 1);
        assert!(effects.contains(&Effect::TurnEnded));
        assert_eq!(t.run, RunState::Idle);

        let items = t.items();
        assert!(matches!(&items[0], Item::User { text, .. } if text == "Hi"));
        assert!(
            matches!(&items[1], Item::Thinking { text, streaming: false, elapsed: Some(_), .. } if text == "Hmm")
        );
        assert!(matches!(&items[2], Item::Text { text, streaming: false, .. } if text == "Hello"));
        let Item::Tool(tool) = &items[3] else {
            panic!("expected a tool call")
        };
        assert_eq!(tool.status, ToolStatus::Done);
        assert_eq!(tool.path(), Some("src/main.rs"));
        assert_eq!(tool.output, "fn main() {}");
        assert_eq!(tool.duration, Some(Duration::from_millis(12)));
    }

    #[test]
    fn bash_failures_report_the_exit_code() {
        let mut t = Transcript::new("/tmp");
        feed(
            &mut t,
            &[
                json!({"type": "tool_execution_start", "toolCallId": "b", "toolName": "bash", "args": {"command": "cargo build"}}),
                json!({"type": "tool_execution_update", "toolCallId": "b", "toolName": "bash", "partialResult": {"content": [{"type": "text", "text": "Compiling"}]}}),
            ],
        );
        assert_eq!(t.tool("b").unwrap().output, "Compiling");
        assert_eq!(t.tool("b").unwrap().status, ToolStatus::Running);
        t.apply(&json!({"type": "tool_execution_end", "toolCallId": "b", "toolName": "bash", "isError": true,
            "result": {"content": [{"type": "text", "text": "error[E0599]\n\nCommand exited with code 101"}]}}));
        let tool = t.tool("b").unwrap();
        assert_eq!(tool.status, ToolStatus::Failed);
        assert_eq!(tool.exit_code(), Some(101));
    }

    #[test]
    fn errors_and_aborts_become_notes() {
        let mut t = Transcript::new("/tmp");
        t.apply(&json!({"type": "message_end", "message": {"role": "assistant", "stopReason": "aborted"}}));
        t.apply(&json!({"type": "message_end", "message": {"role": "assistant", "stopReason": "error", "errorMessage": "rate limited"}}));
        assert_eq!(t.apply(&json!({"type": "agent_settled"})), Effect::Settled);
        assert!(matches!(&t.items()[0], Item::Notice { text, .. } if text == "Stopped by you"));
        assert!(
            matches!(&t.items()[1], Item::Error { text, .. } if text == "Request failed: rate limited")
        );
        assert_eq!(t.run, RunState::Failed);
    }

    #[test]
    fn shows_commands_as_typed() {
        let mut t = Transcript::new("/tmp");
        t.expect_user_text("/skill:pdf-tools extract report.pdf");
        t.apply(&json!({"type": "message_start", "message": {"role": "user",
            "content": "<skill name=\"pdf-tools\">…</skill>\n\nextract report.pdf"}}));
        assert!(
            matches!(&t.items()[0], Item::User { text, .. } if text == "/skill:pdf-tools extract report.pdf")
        );

        t.apply(&json!({"type": "compaction_start", "reason": "manual"}));
        assert!(t.compacting);
        t.apply(&json!({"type": "compaction_end", "result": {"tokensBefore": 182000, "estimatedTokensAfter": 41000}}));
        assert!(!t.compacting);
        assert!(
            matches!(&t.items()[1], Item::Notice { text, .. } if text == "Context compacted · 182k → 41.0k tokens")
        );

        t.clear();
        assert!(t.items().is_empty());
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(format_tokens(48_200), "48.2k");
        assert_eq!(format_tokens(182_000), "182k");
        assert_eq!(format_tokens(950), "950");
        assert_eq!(format_duration(Duration::from_secs(102)), "1m 42s");
        assert_eq!(format_duration(Duration::from_millis(6400)), "6.4s");
    }

    #[test]
    fn restores_skill_messages_as_commands() {
        use super::skill_command;
        let expanded = "<skill name=\"treehouse-commit\" location=\"/d/SKILL.md\">\nReferences are relative to /d.\n\n# Commit\n</skill>\n\nonly the parser";
        assert_eq!(
            skill_command(expanded).as_deref(),
            Some("/commit only the parser")
        );
        let plain = "<skill name=\"pdf-tools\" location=\"/p/SKILL.md\">\nbody\n</skill>";
        assert_eq!(skill_command(plain).as_deref(), Some("/skill:pdf-tools"));
        assert_eq!(skill_command("Fix the test"), None);
    }

    #[test]
    fn restores_saved_commit_cards() {
        let mut t = Transcript::new("/tmp");
        let card = CommitCard {
            id: ItemId::none(),
            commits: vec![crate::git::Commit {
                hash: "029031c".into(),
                subject: "docs: add a cat fact to the README".into(),
                body: String::new(),
                files: 1,
            }],
            base: Some("master".into()),
            ahead: Some(1),
            uncommitted: 0,
        };
        let entries = [
            json!({"type": "message", "message": {"role": "user", "content": "/commit"}}),
            json!({"type": "custom", "customType": COMMITS_ENTRY, "data": serde_json::to_value(&card).unwrap()}),
            json!({"type": "custom", "customType": "someone-else", "data": {}}),
        ];
        t.restore(&entries);
        assert_eq!(t.items().len(), 2);
        let Item::Commits(restored) = &t.items()[1] else {
            panic!("expected a commit card");
        };
        assert_eq!(restored.commits, card.commits);
        assert_eq!(
            (restored.ahead, restored.base.as_deref()),
            (Some(1), Some("master"))
        );
        assert_ne!(restored.id, ItemId::none());
    }

    #[test]
    fn saves_what_pi_does_not_and_restores_it() {
        let mut live = Transcript::new("/tmp");
        live.start_setup(".agents/prepare");
        live.setup_run().unwrap().output.push_str("npm ci\n");
        live.finish_setup(Some(0), false, None);
        live.saved_notice("Pi restarted · settings, skills and MCP servers reloaded");
        live.saved_error("Model not found");
        live.push_mcp_attention(vec![("figma".into(), "needs sign-in".into())]);
        live.sign_in("figma").url = Some("https://figma.com/oauth".into());
        live.finish_sign_in(Some("figma"), SignInState::Done("12 tools".into()));
        // Pi's own notes aren't saved again.
        live.push_notice("Context compacted");
        let saves = live.take_saves();
        assert_eq!(
            saves.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            [
                SETUP_ENTRY,
                NOTICE_ENTRY,
                ERROR_ENTRY,
                MCP_ATTENTION_ENTRY,
                SIGN_IN_ENTRY
            ]
        );

        // Back from the session file, with Pi's own model and thinking changes.
        let mut entries = vec![
            json!({"type": "model_change", "provider": "anthropic", "modelId": "claude-opus-4-5"}),
            json!({"type": "thinking_level_change", "thinkingLevel": "high"}),
        ];
        entries.extend(
            saves
                .into_iter()
                .map(|(kind, data)| json!({"type": "custom", "customType": kind, "data": data})),
        );
        entries.push(json!({"type": "model_change", "provider": "anthropic", "modelId": "claude-sonnet-4-5"}));
        let mut restored = Transcript::new("/tmp");
        restored.restore(&entries);
        let kinds: Vec<&str> = restored
            .items()
            .iter()
            .map(|item| match item {
                Item::Shell(run) if run.setup.is_some() && run.output == "npm ci\n" => "setup",
                Item::Notice { text, .. } if text.starts_with("Pi restarted") => "notice",
                Item::Error { text, .. } if text == "Model not found" => "error",
                Item::McpAttention { servers, .. } if servers[0].0 == "figma" => "attention",
                Item::SignIn(s) if s.state == SignInState::Done("12 tools".into()) => "sign-in",
                Item::Notice { text, .. } if text == "Model changed to claude-sonnet-4-5" => {
                    "model"
                }
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            ["setup", "notice", "error", "attention", "sign-in", "model"]
        );
    }

    #[test]
    fn a_commit_card_follows_the_call_that_committed() {
        let mut t = Transcript::new("/tmp");
        t.ensure_tool("c1", "bash");
        t.ensure_tool("c2", "treehouse_merge");
        let commit = crate::git::Commit {
            hash: "8a7e7d5".into(),
            subject: "feat: say goodbye".into(),
            body: String::new(),
            files: 2,
        };
        t.push_commits(Some("c1"), vec![commit], Some("master".into()), Some(1), 0);
        let kinds: Vec<&str> = t
            .items()
            .iter()
            .map(|item| match item {
                Item::Tool(tool) => tool.call_id.as_str(),
                Item::Commits(_) => "card",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["c1", "card", "c2"]);
    }

    #[test]
    fn a_handed_over_task_shows_once_as_a_card() {
        let task = "Add an about page";
        let mut live = Transcript::new("/tmp");
        live.start_setup(".agents/prepare");
        live.hand_over("master".into(), task.into());
        live.expect_user_text(task);
        live.apply(&json!({"type": "message_start", "message": {"role": "user", "content": task}}));
        let kinds = |t: &Transcript| -> Vec<&'static str> {
            t.items()
                .iter()
                .map(|item| match item {
                    Item::Handover { .. } => "task",
                    Item::Shell(_) => "setup",
                    Item::User { .. } => "user",
                    _ => "other",
                })
                .collect()
        };
        assert_eq!(kinds(&live), ["task", "setup"]);

        // Back from the session file: the entry, then Pi's own message.
        let saves = live.take_saves();
        assert_eq!(saves[0].0, HANDOVER_ENTRY);
        let mut entries: Vec<Value> = saves
            .into_iter()
            .map(|(kind, data)| json!({"type": "custom", "customType": kind, "data": data}))
            .collect();
        entries.push(json!({"type": "message", "message": {"role": "user", "content": task}}));
        let mut restored = Transcript::new("/tmp");
        restored.restore(&entries);
        assert_eq!(kinds(&restored), ["task"]);
    }
}
