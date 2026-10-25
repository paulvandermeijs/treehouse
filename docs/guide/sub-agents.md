# Sub-agents

Pi has no sub-agents of its own. Treehouse adds a `subagent` tool through its [bridge extension](/development/#the-bridge-extension): when Pi calls it, Treehouse starts a separate Pi session in the same worktree to do the task, and returns that session's final answer to Pi as the tool result.

## Following a sub-agent

A sub-agent shows up in two places:

- **In the sidebar**, under its worktree, with its own status dot.
- **In the parent chat**, as a card with its task and progress.

Open it from either to see its chat. <kbd>⌘</kbd>-click it in the sidebar (or **Open in right dock** from its context menu) to follow it in the right dock while you keep the parent chat in the main pane.

![A sub-agent opened in the right dock beside its parent chat](/screens/sub-agent-latte.png){.screen .light-only loading=lazy}
![A sub-agent opened in the right dock beside its parent chat](/screens/sub-agent-mocha.png){.screen .dark-only loading=lazy}

## Steering and stopping

A sub-agent's chat works like any other. Type in its message box (**Steer this sub-agent…**) to send it a message while it works, or stop it with the stop button or **Stop** in its context menu. When a sub-agent finishes or stops, its last reply goes back to the parent as its report.

After a sub-agent has finished you can keep talking to it, though its parent already has its report.

## Model

Sub-agents use the **Sub-agent model** from **Settings › Agent defaults**, or the chat's model when that's empty. A cheaper model is often enough for side quests.

## Approvals

Starting a sub-agent never asks for approval: it only starts another session in the same worktree. The sub-agent's own tool calls go through [approvals](./approvals) like the parent's.
