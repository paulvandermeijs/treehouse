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
    SubAgent { title: SharedString, task: String },
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
    /// A turn ended; the worktree's diff may have changed.
    TurnEnded,
    StatusChanged,
}

pub struct Session {
    pub kind: SessionKind,
    pub cwd: PathBuf,
    pub project: PathBuf,
    client: Option<PiClient>,
    transcript: Transcript,
    texts: HashMap<ItemId, (Entity<TextViewState>, usize)>,
    pub approvals: Vec<PendingApproval>,
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
    /// Pi's saved session, so a restart continues the same conversation.
    session_file: Option<PathBuf>,
    pub started: Instant,
    pub start_error: Option<String>,
    _reader: Option<Task<()>>,
}

impl EventEmitter<SessionEvent> for Session {}

/// Pi's tools that reach MCP tools.
const MCP_DISCOVERY: [&str; 2] = ["codemode", "tool_search"];

/// Tells Pi why it can only read while the folder isn't trusted.
const READ_ONLY_NOTE: &str = "This folder isn't trusted yet, so you only have the read tool: you can't create, edit or delete files or run commands. If the user asks for a change, don't attempt it or claim to have made it. Explain that they need to trust the folder first (the \"Read only\" button at the top of the chat), and say what you would change.";

impl Session {
    pub fn new(kind: SessionKind, cwd: PathBuf, project: PathBuf, cx: &App) -> Self {
        let config = Config::global(cx);
        let model = match &kind {
            SessionKind::SubAgent { .. } => config.subagent_model.clone().or(config.model.clone()),
            SessionKind::Worktree => config.model.clone(),
        };
        Self {
            transcript: Transcript::new(cwd.clone()),
            kind,
            cwd,
            project,
            client: None,
            texts: HashMap::new(),
            approvals: Vec::new(),
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
            session_file: None,
            started: Instant::now(),
            start_error: None,
            _reader: None,
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

    pub fn is_running(&self) -> bool {
        self.transcript.run == RunState::Running
    }

    pub fn status(&self) -> AgentStatus {
        if !self.approvals.is_empty() || self.waiting_sign_in().is_some() {
            AgentStatus::NeedsInput
        } else if self.transcript.run == RunState::Running {
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

    fn ensure_started(&mut self, cx: &mut Context<Self>) -> bool {
        if self.client.is_some() {
            return true;
        }
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
                    .chain(MCP_DISCOVERY.iter().map(|t| t.to_string()))
                    .collect()
            } else {
                vec!["read".to_string()]
            },
            extensions: vec![extension],
            session_dir: Some(Config::data_dir().join("sessions")),
            session_file: self.session_file.clone(),
            approve: Some(self.trusted),
            // Without this, a model asked to change something tends to try
            // tools it doesn't have and then report success anyway.
            // Read-only means no MCP servers either: their tools can change
            // things outside the folder.
            mcp: self.trusted,
            system_note: (!self.trusted).then(|| READ_ONLY_NOTE.to_string()),
        };
        match PiClient::spawn(&options) {
            Ok((client, messages)) => {
                client.send(command::get_state());
                client.send(command::get_available_models());
                client.send(command::get_commands());
                self.client = Some(client);
                self.start_error = None;
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

    fn fail_to_start(&mut self, error: String, cx: &mut Context<Self>) {
        self.transcript
            .push_error(format!("Couldn't start Pi. {error}"));
        self.start_error = Some(error);
        cx.emit(SessionEvent::StatusChanged);
        cx.notify();
    }

    /// Sends a message; while Pi is working it steers the current run.
    /// `/compact` and `/new` run as commands instead.
    pub fn send(&mut self, message: &str, cx: &mut Context<Self>) {
        let message = message.trim();
        if message.is_empty() || !self.ensure_started(cx) {
            return;
        }
        match crate::slash::parse_builtin(message) {
            Some(crate::slash::Builtin::Compact(instructions)) => {
                return self.compact(instructions.as_deref(), cx);
            }
            Some(crate::slash::Builtin::New) => return self.new_session(cx),
            None => {}
        }
        let streaming = self.is_running();
        if let Some(client) = &self.client {
            // `/mcp …` is an extension command: Pi runs it without a message.
            if !message.starts_with("/mcp") {
                self.transcript.expect_user_text(message);
            }
            client.send(command::prompt(message, streaming));
        }
        self.viewed = true;
        cx.notify();
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

    /// Applies a trust decision. A running Pi restarts on the same saved
    /// session, since its tools and project resources are fixed at launch.
    pub fn set_trusted(&mut self, trusted: bool, cx: &mut Context<Self>) {
        if self.trusted == trusted {
            return;
        }
        self.trusted = trusted;
        if self.client.is_none() {
            return cx.notify();
        }
        self._reader = None;
        self.client = None;
        self.approvals.clear();
        self.transcript
            .apply(&serde_json::json!({"type": "agent_settled"}));
        self.transcript.push_notice(if trusted {
            "Folder trusted · Pi restarted with full access"
        } else {
            "Read-only · Pi restarted with the read tool only"
        });
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

    fn handle(&mut self, message: PiMessage, cx: &mut Context<Self>) {
        match message {
            PiMessage::Response(response) => self.handle_response(response),
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
                    self.transcript
                        .push_error(format!("Pi exited (code {}). {detail}", code.unwrap_or(-1)));
                    self.transcript
                        .apply(&serde_json::json!({"type": "agent_settled"}));
                }
                self.subagent_requests.clear();
                self.approvals.clear();
                cx.emit(SessionEvent::StatusChanged);
            }
        }
        self.sync_texts(cx);
        cx.notify();
    }

    fn handle_response(&mut self, response: crate::pi::rpc::Response) {
        if !response.success {
            if response.command == "compact" {
                self.transcript.compacting = false;
            }
            if let Some(error) = response.error {
                self.transcript.push_error(error);
            }
            return;
        }
        let data = &response.data;
        match response.command.as_str() {
            "get_state" => {
                if let Some(file) = data.get("sessionFile").and_then(Value::as_str) {
                    self.session_file = Some(PathBuf::from(file));
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
                self.transcript.push_notice("New session");
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
                    if let Some(client) = &self.client {
                        client.send(command::get_session_stats());
                    }
                }
                Effect::None => {}
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
                        self.transcript.push_error(message);
                    }
                } else {
                    self.transcript.push_notice(message);
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
            let Item::Text { id, text, .. } = item else {
                continue;
            };
            match self.texts.get_mut(id) {
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
