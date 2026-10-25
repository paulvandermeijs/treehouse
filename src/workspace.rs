//! Projects, their worktrees and the sub-agents running in each worktree.

use anyhow::Result;
use gpui_kit::{
    AppContext as _, Context, Entity, EntityId, EventEmitter, SharedString, Subscription,
};
use std::path::{Path, PathBuf};

use crate::config::{CompareBase, Config};
use crate::git::{self, DiffStat};
use crate::session::{AgentStatus, Session, SessionEvent, SessionKind};
use crate::trust::Trust;

pub struct SubAgent {
    pub session: Entity<Session>,
    pub parent: Entity<Session>,
    pub request_id: String,
    pub tool_call_id: String,
    reported: bool,
    _subscription: Subscription,
}

pub struct Worktree {
    pub path: PathBuf,
    pub branch: SharedString,
    pub base: String,
    pub is_main: bool,
    pub session: Entity<Session>,
    pub subagents: Vec<SubAgent>,
    pub stat: DiffStat,
    _subscription: Subscription,
}

impl Worktree {
    pub fn status(&self, cx: &gpui_kit::App) -> AgentStatus {
        AgentStatus::most_urgent(
            std::iter::once(self.session.read(cx).status())
                .chain(self.subagents.iter().map(|s| s.session.read(cx).status())),
        )
    }
}

pub struct Project {
    pub path: PathBuf,
    pub name: SharedString,
    pub worktrees: Vec<Worktree>,
    pub error: Option<String>,
    pub trust: Trust,
}

pub enum WorkspaceEvent {
    /// A worktree's diff changed after an agent turn.
    DiffChanged(PathBuf),
}

pub struct Workspace {
    pub projects: Vec<Project>,
}

impl EventEmitter<WorkspaceEvent> for Workspace {}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            projects: Vec::new(),
        };
        for path in Config::global(cx).projects.clone() {
            this.load_project(path, cx);
        }
        this
    }

    /// Adds a folder as a project. A folder that isn't a repository yet gets
    /// `git init`, so it has a worktree to chat in.
    pub fn add_project(&mut self, path: PathBuf, cx: &mut Context<Self>) -> Result<()> {
        if self.projects.iter().any(|p| p.path == path) {
            return Ok(());
        }
        if !git::is_repository(&path) {
            git::init_repository(&path)?;
        }
        Config::update(cx, |config| {
            if !config.projects.contains(&path) {
                config.projects.push(path.clone());
            }
        });
        self.load_project(path, cx);
        cx.notify();
        Ok(())
    }

    pub fn remove_project(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.projects.retain(|p| p.path != path);
        Config::update(cx, |config| config.projects.retain(|p| p != path));
        cx.notify();
    }

    fn load_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let mut project = Project {
            path: path.clone(),
            name: name.into(),
            worktrees: Vec::new(),
            trust: crate::trust::lookup(&path),
            error: None,
        };
        match git::list_worktrees(&path) {
            Ok(list) => {
                let base = self.base_for(&path, cx);
                for (ix, info) in list.into_iter().enumerate() {
                    let worktree = self.make_worktree(
                        &path,
                        info.path,
                        info.branch,
                        base.clone(),
                        ix == 0,
                        cx,
                    );
                    project.worktrees.push(worktree);
                }
            }
            Err(err) => project.error = Some(format!("{err:#}")),
        }
        self.projects.push(project);
        let paths: Vec<PathBuf> = self
            .projects
            .last()
            .unwrap()
            .worktrees
            .iter()
            .map(|w| w.path.clone())
            .collect();
        for path in paths {
            self.refresh_stat(path, cx);
        }
    }

    fn base_for(&self, project: &Path, cx: &Context<Self>) -> String {
        match Config::global(cx).compare_base {
            CompareBase::CreatedFrom => git::default_base(project),
            CompareBase::OriginMain => "origin/main".to_string(),
        }
    }

    fn make_worktree(
        &self,
        project: &Path,
        path: PathBuf,
        branch: String,
        base: String,
        is_main: bool,
        cx: &mut Context<Self>,
    ) -> Worktree {
        let trusted = crate::trust::lookup(project).is_trusted();
        let session = cx.new(|cx| {
            let mut session = Session::new(
                SessionKind::Worktree,
                path.clone(),
                project.to_path_buf(),
                cx,
            );
            session.trusted = trusted;
            session
        });
        let subscription = cx.subscribe(&session, Self::on_session_event);
        Worktree {
            path,
            branch: branch.into(),
            base,
            is_main,
            session,
            subagents: Vec::new(),
            stat: DiffStat::default(),
            _subscription: subscription,
        }
    }

    /// Creates a worktree with a new branch and runs the setup command.
    pub fn new_worktree(
        &mut self,
        project: &Path,
        branch: &str,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        let config = Config::global(cx);
        let location = git::worktree_location(&config.worktree_location, project, branch);
        // The setup command is the project's own code: only with trust.
        let setup = if crate::trust::lookup(project).is_trusted() {
            config.setup_command.clone()
        } else {
            String::new()
        };
        git::add_worktree(project, branch, &location)?;
        if !setup.trim().is_empty() {
            let location = location.clone();
            cx.background_spawn(async move {
                let _ = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(setup)
                    .current_dir(location)
                    .status();
            })
            .detach();
        }
        let base = git::default_base(project);
        let worktree = self.make_worktree(
            project,
            location.clone(),
            branch.to_string(),
            base,
            false,
            cx,
        );
        if let Some(project) = self.projects.iter_mut().find(|p| p.path == project) {
            project.worktrees.push(worktree);
        }
        cx.notify();
        Ok(location)
    }

    /// Removes a worktree, and its branch when Settings say so. Without
    /// `force` git refuses to drop changed files or unmerged commits.
    pub fn delete_worktree(
        &mut self,
        path: &Path,
        force: bool,
        cx: &mut Context<Self>,
    ) -> Result<Option<String>> {
        let delete = Config::global(cx).delete_on_remove;
        for project in self.projects.iter_mut() {
            if let Some(ix) = project
                .worktrees
                .iter()
                .position(|w| w.path == path && !w.is_main)
            {
                let branch = project.worktrees[ix].branch.to_string();
                let note = git::remove_worktree(
                    &project.path,
                    path,
                    delete.then_some(branch.as_str()),
                    force,
                )?;
                let worktree = project.worktrees.remove(ix);
                worktree.session.update(cx, |s, cx| s.stop(cx));
                crate::history::forget(path);
                cx.notify();
                return Ok(note);
            }
        }
        Ok(None)
    }

    pub fn rename_branch(&mut self, path: &Path, name: &str, cx: &mut Context<Self>) -> Result<()> {
        git::rename_branch(path, name)?;
        if let Some(worktree) = self.worktree_mut(path) {
            worktree.branch = name.to_string().into();
        }
        cx.notify();
        Ok(())
    }

    /// Saves a trust decision and applies it to every project it covers
    /// (trusting a parent folder can cover several).
    pub fn set_trust(
        &mut self,
        project: &Path,
        decision: crate::trust::Decision,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        crate::trust::save(project, decision)?;
        for project in self.projects.iter_mut() {
            project.trust = crate::trust::lookup(&project.path);
            let trusted = project.trust.is_trusted();
            for worktree in project.worktrees.iter() {
                worktree
                    .session
                    .update(cx, |s, cx| s.set_trusted(trusted, cx));
                for subagent in worktree.subagents.iter() {
                    subagent
                        .session
                        .update(cx, |s, cx| s.set_trusted(trusted, cx));
                }
            }
        }
        cx.notify();
        Ok(())
    }

    pub fn worktree(&self, path: &Path) -> Option<&Worktree> {
        self.projects
            .iter()
            .flat_map(|p| p.worktrees.iter())
            .find(|w| w.path == path)
    }

    fn worktree_mut(&mut self, path: &Path) -> Option<&mut Worktree> {
        self.projects
            .iter_mut()
            .flat_map(|p| p.worktrees.iter_mut())
            .find(|w| w.path == path)
    }

    pub fn project_of(&self, worktree: &Path) -> Option<&Project> {
        self.projects
            .iter()
            .find(|p| p.worktrees.iter().any(|w| w.path == worktree))
    }

    /// The worktree a session belongs to, its own chat or a sub-agent's.
    pub fn worktree_of(&self, session: EntityId) -> Option<&Worktree> {
        self.projects
            .iter()
            .flat_map(|p| p.worktrees.iter())
            .find(|w| {
                w.session.entity_id() == session
                    || w.subagents.iter().any(|s| s.session.entity_id() == session)
            })
    }

    /// The sub-agent started by a parent's `subagent` tool call.
    pub fn subagent_for_call(&self, tool_call_id: &str) -> Option<&SubAgent> {
        self.projects
            .iter()
            .flat_map(|p| p.worktrees.iter())
            .flat_map(|w| w.subagents.iter())
            .find(|s| s.tool_call_id == tool_call_id)
    }

    fn on_session_event(
        &mut self,
        session: Entity<Session>,
        event: &SessionEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SessionEvent::SubAgentRequested {
                request_id,
                tool_call_id,
                title,
                task,
            } => {
                let (cwd, project) = {
                    let s = session.read(cx);
                    (s.cwd.clone(), s.project.clone())
                };
                let kind = SessionKind::SubAgent {
                    title: title.clone().into(),
                    task: task.clone(),
                };
                // Without a sub-agent model setting, a sub-agent uses whatever
                // its parent runs on, including a model picked in the chat.
                let (parent_model, parent_thinking, parent_trusted) = {
                    let s = session.read(cx);
                    (s.model.clone(), s.thinking.clone(), s.trusted)
                };
                let inherit = Config::global(cx).subagent_model.is_none();
                let child = cx.new(|cx| {
                    let mut child = Session::new(kind, cwd.clone(), project, cx);
                    child.trusted = parent_trusted;
                    if inherit {
                        child.model = parent_model.or(child.model);
                        child.thinking = parent_thinking;
                    }
                    child
                });
                let subscription = cx.subscribe(&child, Self::on_session_event);
                let task = task.clone();
                child.update(cx, |child, cx| child.send(&task, cx));
                if let Some(worktree) = self.worktree_mut(&cwd) {
                    worktree.subagents.push(SubAgent {
                        session: child,
                        parent: session.clone(),
                        request_id: request_id.clone(),
                        tool_call_id: tool_call_id.clone(),
                        reported: false,
                        _subscription: subscription,
                    });
                }
            }
            SessionEvent::TurnEnded => {
                let cwd = session.read(cx).cwd.clone();
                self.refresh_stat(cwd, cx);
            }
            SessionEvent::StatusChanged => self.report_finished_subagents(cx),
        }
        cx.notify();
    }

    /// Hands a finished sub-agent's report to the tool call that started it.
    fn report_finished_subagents(&mut self, cx: &mut Context<Self>) {
        let mut reports = Vec::new();
        for worktree in self
            .projects
            .iter_mut()
            .flat_map(|p| p.worktrees.iter_mut())
        {
            for subagent in worktree.subagents.iter_mut() {
                if subagent.reported {
                    continue;
                }
                let child = subagent.session.read(cx);
                let done = matches!(
                    child.status(),
                    AgentStatus::Finished | AgentStatus::Idle | AgentStatus::Failed
                ) && !child.is_running()
                    && !child.items().is_empty();
                if done {
                    subagent.reported = true;
                    let report = child.last_assistant_text().or_else(|| {
                        child
                            .start_error
                            .clone()
                            .map(|e| format!("The sub-agent failed to start: {e}"))
                    });
                    reports.push((subagent.parent.clone(), subagent.request_id.clone(), report));
                }
            }
        }
        for (parent, request_id, report) in reports {
            parent.update(cx, |parent, cx| {
                parent.finish_subagent(&request_id, report, cx)
            });
        }
    }

    pub fn refresh_stat(&mut self, worktree: PathBuf, cx: &mut Context<Self>) {
        let Some(base) = self.worktree(&worktree).map(|w| w.base.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let path = worktree.clone();
            let stat = cx
                .background_spawn(async move {
                    git::diff_since(&path, &base).map(|patch| git::diff_stat(&patch))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let (Some(worktree), Ok(stat)) = (this.worktree_mut(&worktree), stat) {
                    worktree.stat = stat;
                }
                cx.emit(WorkspaceEvent::DiffChanged(worktree));
                cx.notify();
            });
        })
        .detach();
    }
}
