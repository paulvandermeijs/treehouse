//! Projects, their worktrees and the sub-agents running in each worktree.

use anyhow::Result;
use gpui_kit::{
    AppContext as _, Context, Entity, EntityId, EventEmitter, SharedString, Subscription,
};
use std::collections::{HashMap, HashSet};
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
    pub state: git::BranchState,
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
    /// Worktrees whose stat is being read, and those asked for again
    /// meanwhile: they get one more read once it's done, so the last
    /// request, such as the end of a turn, always shows.
    refreshing: HashSet<PathBuf>,
    refresh_again: HashSet<PathBuf>,
    /// The commit each base branch was at, by project and base: when it
    /// moves, every worktree compared with it is behind by more.
    base_heads: HashMap<(PathBuf, String), String>,
}

impl EventEmitter<WorkspaceEvent> for Workspace {}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            projects: Vec::new(),
            refreshing: HashSet::new(),
            refresh_again: HashSet::new(),
            base_heads: HashMap::new(),
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
        Config::update(cx, |config| {
            config.projects.retain(|p| p != path);
            config.pinned.retain(|p| p != path);
        });
        cx.notify();
    }

    /// Projects as the sidebar lists them: pinned ones first, in the order
    /// they were pinned, then the rest in the order they were added.
    pub fn ordered(&self, cx: &gpui_kit::App) -> Vec<&Project> {
        let pinned = &Config::global(cx).pinned;
        let mut projects: Vec<&Project> = self.projects.iter().collect();
        projects.sort_by_key(|p| pin_rank(pinned, &p.path));
        projects
    }

    /// Pins a project to the top, after those already pinned, or unpins it.
    pub fn toggle_pin(&mut self, path: &Path, cx: &mut Context<Self>) {
        Config::update(cx, |config| {
            if config.pinned.iter().any(|p| p == path) {
                config.pinned.retain(|p| p != path);
            } else {
                config.pinned.push(path.to_path_buf());
            }
        });
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
            session.base = (!is_main).then(|| base.clone());
            session
        });
        let subscription = cx.subscribe(&session, Self::on_session_event);
        let subagents = Self::restore_subagents(&session, &path, project, trusted, cx);
        Worktree {
            path,
            branch: branch.into(),
            base,
            is_main,
            session,
            subagents,
            stat: DiffStat::default(),
            state: git::BranchState::default(),
            _subscription: subscription,
        }
    }

    /// The sub-agents the worktree's saved session started, back as they
    /// were once finished: their own chat, with nothing left to report.
    fn restore_subagents(
        parent: &Entity<Session>,
        worktree: &Path,
        project: &Path,
        trusted: bool,
        cx: &mut Context<Self>,
    ) -> Vec<SubAgent> {
        let Some(saved) = crate::history::saved(worktree) else {
            return Vec::new();
        };
        saved
            .subagents
            .into_iter()
            .map(|saved| {
                let kind = SessionKind::SubAgent {
                    title: saved.title.into(),
                    task: saved.task,
                    tool_call_id: saved.tool_call_id.clone(),
                };
                let session = cx.new(|cx| {
                    let mut session =
                        Session::new(kind, worktree.to_path_buf(), project.to_path_buf(), cx);
                    session.trusted = trusted;
                    session.resume(saved.file);
                    session
                });
                SubAgent {
                    _subscription: cx.subscribe(&session, Self::on_session_event),
                    session,
                    parent: parent.clone(),
                    request_id: String::new(),
                    tool_call_id: saved.tool_call_id,
                    reported: true,
                }
            })
            .collect()
    }

    /// Creates a worktree with a new branch and prepares it with the project's `.agents/` setup.
    pub fn new_worktree(
        &mut self,
        project: &Path,
        branch: &str,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        let config = Config::global(cx);
        let location = git::worktree_location(&config.worktree_location, project, branch);
        git::add_worktree(project, branch, &location)?;
        let base = git::default_base(project);
        let worktree = self.make_worktree(
            project,
            location.clone(),
            branch.to_string(),
            base,
            false,
            cx,
        );
        // The project's .agents/ setup runs in the new worktree's chat; it
        // waits there when the project isn't trusted yet.
        let session = worktree.session.clone();
        if let Some(project) = self.projects.iter_mut().find(|p| p.path == project) {
            project.worktrees.push(worktree);
        }
        session.update(cx, |session, cx| session.run_setup(cx));
        cx.notify();
        Ok(location)
    }

    /// Removes a worktree, and its branch when Settings say so. Without
    /// `force` git refuses to drop changed files or unmerged commits.
    /// `clean_up` is for a merged worktree: its branch goes too, whatever the
    /// setting, since its work is in the base.
    pub fn delete_worktree(
        &mut self,
        path: &Path,
        force: bool,
        clean_up: bool,
        cx: &mut Context<Self>,
    ) -> Result<Option<String>> {
        let delete = clean_up || Config::global(cx).delete_on_remove;
        for project in self.projects.iter_mut() {
            if let Some(ix) = project
                .worktrees
                .iter()
                .position(|w| w.path == path && !w.is_main)
            {
                let branch = project.worktrees[ix].branch.to_string();
                let base = project.worktrees[ix].base.clone();
                let note = git::remove_worktree(
                    &project.path,
                    path,
                    delete.then_some(branch.as_str()),
                    force,
                    clean_up.then_some(base.as_str()),
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
                    tool_call_id: tool_call_id.clone(),
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
            SessionEvent::MergeRequested {
                request_id,
                tool_call_id,
                squash_message,
            } => self.check_merge(
                session,
                request_id.clone(),
                tool_call_id.clone(),
                squash_message.clone(),
                cx,
            ),
            SessionEvent::WorktreeRequested {
                request_id,
                branch,
                task,
            } => self.create_for_pi(session, request_id.clone(), branch, task.clone(), cx),
            SessionEvent::MergeApproved(merge, choice) => {
                self.merge(session, merge.clone(), *choice, cx)
            }
            SessionEvent::TurnEnded => {
                let cwd = session.read(cx).cwd.clone();
                self.refresh_after_turn(cwd, cx);
            }
            SessionEvent::StatusChanged => self.report_finished_subagents(cx),
        }
        cx.notify();
    }

    /// Creates the worktree Pi asked for, the way New worktree does, and
    /// hands it the task, which its own chat then works on.
    fn create_for_pi(
        &mut self,
        session: Entity<Session>,
        request_id: String,
        branch: &str,
        task: String,
        cx: &mut Context<Self>,
    ) {
        let asking = self.worktree_of(session.entity_id()).map(|w| {
            let project = self.project_of(&w.path).map(|p| p.path.clone());
            (project, w.branch.to_string())
        });
        let Some((Some(project), from)) = asking else {
            return;
        };
        let branch = branch.trim();
        let created = if branch.is_empty() {
            Err("Give the new worktree a branch name.".to_string())
        } else {
            self.new_worktree(&project, branch, cx)
                .map_err(|err| format!("Couldn't create the worktree: {err:#}"))
        };
        let outcome = created.map(|path| {
            let handed = !task.trim().is_empty();
            if handed
                && let Some(new) = self.worktree(&path).map(|w| w.session.clone())
            {
                new.update(cx, |s, cx| s.hand_over(from.clone(), task.clone(), cx));
            }
            let text = if handed {
                format!(
                    "Created the worktree {branch} at {}. Its own chat is working on the task; don't edit its files from here.",
                    path.display()
                )
            } else {
                format!("Created the worktree {branch} at {}. Its chat is ready.", path.display())
            };
            let worktree = serde_json::json!({
                "branch": branch,
                "path": path,
                "task": handed,
            });
            (text, worktree)
        });
        session.update(cx, |s, cx| s.finish_worktree(&request_id, outcome, cx));
    }

    /// Checks a merge Pi asked for, then shows its approval card or tells Pi
    /// why it can't happen.
    fn check_merge(
        &mut self,
        session: Entity<Session>,
        request_id: String,
        tool_call_id: String,
        squash_message: String,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self.worktree_of(session.entity_id()) else {
            return;
        };
        let (path, branch, base) = (
            worktree.path.clone(),
            worktree.branch.to_string(),
            worktree.base.clone(),
        );
        let Some(project) = self.project_of(&path) else {
            return;
        };
        let project_path = project.path.clone();
        // The base's own chat may be changing the same files.
        let busy = project
            .worktrees
            .iter()
            .any(|w| w.branch.as_ref() == base && w.status(cx) == AgentStatus::Working);
        if busy {
            let reason =
                format!("An agent is working in {base}'s worktree. Wait for it to finish.");
            return session.update(cx, |s, cx| s.finish_merge(&request_id, Err(reason), cx));
        }
        cx.spawn(async move |_, cx| {
            let checked = cx
                .background_spawn({
                    let (branch, base) = (branch.clone(), base.clone());
                    async move { git::check_merge(&project_path, &path, &branch, &base) }
                })
                .await;
            session.update(cx, |s, cx| match checked {
                Ok(plan) => s.ask_merge(
                    crate::session::PendingMerge {
                        request_id,
                        tool_call_id,
                        branch,
                        base,
                        plan,
                        squash_message,
                    },
                    cx,
                ),
                Err(reason) => s.finish_merge(&request_id, Err(reason), cx),
            });
        })
        .detach();
    }

    /// Carries out a merge the user approved, checking again first since
    /// either folder may have changed while the card waited.
    fn merge(
        &mut self,
        session: Entity<Session>,
        merge: crate::session::PendingMerge,
        choice: crate::session::MergeChoice,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self.worktree_of(session.entity_id()) else {
            return;
        };
        let path = worktree.path.clone();
        let Some(project) = self.project_of(&path).map(|p| p.path.clone()) else {
            return;
        };
        let squash =
            (choice == crate::session::MergeChoice::Squash).then(|| merge.squash_message.clone());
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn({
                    let (path, merge) = (path.clone(), merge.clone());
                    async move {
                        let plan = git::check_merge(&project, &path, &merge.branch, &merge.base)?;
                        let sha = git::merge_into(&path, &merge.base, &plan, squash.as_deref())
                            .map_err(|err| format!("{err:#}"))?;
                        let (base, branch, commits) = (&merge.base, &merge.branch, plan.commits);
                        let text = match &squash {
                            Some(_) => format!(
                                "Squashed {branch} into one commit on {base} ({sha}). Not pushed."
                            ),
                            None => {
                                let commits = match commits {
                                    1 => "1 commit".to_string(),
                                    n => format!("{n} commits"),
                                };
                                format!(
                                    "Merged {branch} into {base} with a fast-forward ({commits}); {base} is now at {sha}. Not pushed."
                                )
                            }
                        };
                        let merged = serde_json::json!({
                            "base": base,
                            "branch": branch,
                            "sha": sha,
                            "commits": commits,
                            "squash": squash.is_some(),
                        });
                        Ok((text, merged))
                    }
                })
                .await;
            session.update(cx, |s, cx| s.finish_merge(&merge.request_id, outcome, cx));
            // The base's folder changed too.
            let _ = this.update(cx, |this, cx| this.refresh_project(&path, cx));
        })
        .detach();
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
                // A run has to have ended: a sub-agent whose Pi is still
                // starting can already show a notice, such as MCP servers
                // needing attention, and would report back empty.
                let ended = child.transcript_settled_at().is_some()
                    || child.start_error.is_some()
                    || child.exited.is_some();
                let done = matches!(
                    child.status(),
                    AgentStatus::Finished | AgentStatus::Idle | AgentStatus::Failed
                ) && !child.is_running()
                    && ended;
                if done {
                    subagent.reported = true;
                    let report = child.last_assistant_text().or_else(|| {
                        child
                            .start_error
                            .clone()
                            .map(|e| format!("The sub-agent failed to start: {e}"))
                            .or_else(|| child.exited.clone())
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

    /// Undoes a sync Pi left halfway: the one lifecycle step done by
    /// button, since it only undoes.
    pub fn abort_sync(&mut self, worktree: &Path, cx: &mut Context<Self>) -> Result<()> {
        git::abort_sync(worktree)?;
        self.refresh_stat(worktree.to_path_buf(), cx);
        Ok(())
    }

    /// Refreshes every worktree in the project of `worktree`: a commit on
    /// the base branch changes how far behind each of the others is.
    pub fn refresh_project(&mut self, worktree: &Path, cx: &mut Context<Self>) {
        if let Some(project) = self.project_of(worktree).map(|p| p.path.clone()) {
            self.sync_worktrees(project, cx);
        }
        let paths: Vec<PathBuf> = self
            .project_of(worktree)
            .map(|p| p.worktrees.iter().map(|w| w.path.clone()).collect())
            .unwrap_or_default();
        for path in paths {
            self.refresh_stat(path, cx);
        }
    }

    /// After a turn, or a step of one: its own worktree, any worktree made
    /// meanwhile, and the others only if a base branch moved.
    fn refresh_after_turn(&mut self, worktree: PathBuf, cx: &mut Context<Self>) {
        let Some(project) = self.project_of(&worktree).map(|p| p.path.clone()) else {
            return;
        };
        self.sync_worktrees(project.clone(), cx);
        self.refresh_stat(worktree.clone(), cx);
        let bases: Vec<String> = self
            .project_of(&worktree)
            .map(|p| {
                p.worktrees
                    .iter()
                    .map(|w| w.base.clone())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default()
            .into_iter()
            .collect();
        cx.spawn(async move |this, cx| {
            let heads = cx
                .background_spawn({
                    let project = project.clone();
                    async move {
                        bases
                            .into_iter()
                            .filter_map(|base| Some((git::rev_parse(&project, &base)?, base)))
                            .collect::<Vec<_>>()
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let moved: Vec<String> = heads
                    .into_iter()
                    .filter_map(|(head, base)| {
                        let key = (project.clone(), base.clone());
                        (this.base_heads.insert(key, head.clone()).as_ref() != Some(&head))
                            .then_some(base)
                    })
                    .collect();
                let others: Vec<PathBuf> = this
                    .projects
                    .iter()
                    .filter(|p| p.path == project)
                    .flat_map(|p| p.worktrees.iter())
                    .filter(|w| w.path != worktree && moved.contains(&w.base))
                    .map(|w| w.path.clone())
                    .collect();
                for path in others {
                    this.refresh_stat(path, cx);
                }
            });
        })
        .detach();
    }

    /// Picks up worktrees made outside Treehouse, by Pi or in a terminal, and
    /// branches renamed there, by reading git's worktree list again.
    fn sync_worktrees(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_spawn({
                    let project = project.clone();
                    async move { git::list_worktrees(&project) }
                })
                .await;
            let Ok(listed) = listed else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                let Some(known) = this.projects.iter().find(|p| p.path == project).map(|p| {
                    p.worktrees
                        .iter()
                        .map(|w| (w.path.clone(), w.branch.to_string()))
                        .collect::<Vec<_>>()
                }) else {
                    return;
                };
                let (added, renamed) = sync_plan(&known, listed);
                if added.is_empty() && renamed.is_empty() {
                    return;
                }
                for (path, branch) in renamed {
                    if let Some(worktree) = this.worktree_mut(&path) {
                        worktree.branch = branch.into();
                    }
                }
                let base = this.base_for(&project, cx);
                let mut paths = Vec::new();
                for info in added {
                    paths.push(info.path.clone());
                    let worktree = this.make_worktree(
                        &project,
                        info.path,
                        info.branch,
                        base.clone(),
                        false,
                        cx,
                    );
                    if let Some(p) = this.projects.iter_mut().find(|p| p.path == project) {
                        p.worktrees.push(worktree);
                    }
                }
                for path in paths {
                    this.refresh_stat(path, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the worktree's diff stat and branch state again.
    pub fn refresh_stat(&mut self, worktree: PathBuf, cx: &mut Context<Self>) {
        let Some((base, branch, is_main)) = self
            .worktree(&worktree)
            .map(|w| (w.base.clone(), w.branch.to_string(), w.is_main))
        else {
            return;
        };
        if self.refreshing.contains(&worktree) {
            self.refresh_again.insert(worktree);
            return;
        }
        self.refreshing.insert(worktree.clone());
        cx.spawn(async move |this, cx| {
            let path = worktree.clone();
            let (stat, state) = cx
                .background_spawn(async move {
                    let stat = git::diff_stat_since(&path, &base);
                    let base = (!is_main).then_some(base.as_str());
                    (stat, git::branch_state(&path, &branch, base))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(worktree) = this.worktree_mut(&worktree) {
                    if let Ok(stat) = stat {
                        worktree.stat = stat;
                    }
                    worktree.state = state;
                }
                this.refreshing.remove(&worktree);
                if this.refresh_again.remove(&worktree) {
                    this.refresh_stat(worktree.clone(), cx);
                }
                cx.emit(WorkspaceEvent::DiffChanged(worktree));
                cx.notify();
            });
        })
        .detach();
    }
}

/// What git's worktree list has that Treehouse doesn't: worktrees to add,
/// and known ones whose branch was renamed. Paths are compared resolved,
/// since git prints them without symlinks. A worktree git no longer lists
/// stays until Treehouse deletes it, so a chat isn't pulled from under you.
fn sync_plan(
    known: &[(PathBuf, String)],
    listed: Vec<git::WorktreeInfo>,
) -> (Vec<git::WorktreeInfo>, Vec<(PathBuf, String)>) {
    let resolve = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let known: Vec<(PathBuf, &PathBuf, &String)> = known
        .iter()
        .map(|(path, branch)| (resolve(path), path, branch))
        .collect();
    let mut added = Vec::new();
    let mut renamed = Vec::new();
    for info in listed {
        let here = resolve(&info.path);
        match known.iter().find(|(resolved, ..)| *resolved == here) {
            Some((_, path, branch)) if **branch != info.branch => {
                renamed.push(((*path).clone(), info.branch))
            }
            Some(_) => {}
            None => added.push(info),
        }
    }
    (added, renamed)
}

/// Where a project goes among the pinned ones; unpinned ones come after, in
/// their own order (the sort is stable).
fn pin_rank(pinned: &[PathBuf], path: &Path) -> usize {
    pinned.iter().position(|p| p == path).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{pin_rank, sync_plan};
    use crate::git::WorktreeInfo;

    #[test]
    fn picks_up_worktrees_made_elsewhere() {
        let known = vec![
            (PathBuf::from("/code/app"), "main".to_string()),
            (
                PathBuf::from("/code/app/.worktrees/a"),
                "feat/a".to_string(),
            ),
        ];
        let listed = vec![
            WorktreeInfo {
                path: "/code/app".into(),
                branch: "main".into(),
            },
            WorktreeInfo {
                path: "/code/app/.worktrees/a".into(),
                branch: "feat/a2".into(),
            },
            WorktreeInfo {
                path: "/code/app/.worktrees/about".into(),
                branch: "about-page".into(),
            },
        ];
        let (added, renamed) = sync_plan(&known, listed);
        assert_eq!(
            added.iter().map(|w| w.branch.as_str()).collect::<Vec<_>>(),
            ["about-page"]
        );
        assert_eq!(
            renamed,
            [(
                PathBuf::from("/code/app/.worktrees/a"),
                "feat/a2".to_string()
            )]
        );
    }
    use std::path::PathBuf;

    #[test]
    fn pinned_first_in_pin_order() {
        let added: Vec<PathBuf> = ["a", "b", "c", "d"].iter().map(PathBuf::from).collect();
        let pinned: Vec<PathBuf> = ["c", "a"].iter().map(PathBuf::from).collect();
        let mut order = added.clone();
        order.sort_by_key(|p| pin_rank(&pinned, p));
        let names: Vec<_> = order.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(names, ["c", "a", "b", "d"]);
    }
}
