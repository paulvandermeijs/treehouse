# The chat

Every worktree has its own chat with Pi. Pi starts the first time you send a message in a worktree; each chat is a separate `pi --mode rpc` process working in that worktree's folder.

![A worktree chat: tool calls, two sub-agent cards and the toolbar under the message box](/screens/chat-latte.png){.screen .light-only loading=lazy}
![A worktree chat: tool calls, two sub-agent cards and the toolbar under the message box](/screens/chat-mocha.png){.screen .dark-only loading=lazy}

The chat's header shows the branch, where Pi stands and the worktree's diff stat. Once the chat has had a turn, a pill after the branch name says **Running**, **Needs input**, **Failed** or **Finished**, as a sub-agent's header does. Finished means Pi's last turn ended; unlike the sidebar's dot, it stays after you've seen the chat.

## Sending and steering

Type in the message box and press <kbd>Enter</kbd>. While Pi is working, the send button turns into **Steer**: a message you send then goes into the current run instead of waiting for it to finish.

<kbd>⌘.</kbd> (<kbd>Ctrl+.</kbd>) or the stop button stops the agent.

## Model and thinking level

The toolbar under the message box shows the chat's model and thinking level. Click either to change it for this chat, or use <kbd>⌘M</kbd> and **Set thinking level…** in the command palette.

The model picker lists the models of every provider you have a key for:

- **Favorites** first, then **Recent**: the last three models you picked that aren't favorites.
- Then a group per provider. With more than 20 models, only the current model's provider starts open; click a heading to open or close it.
- Providers that sell other makers' models, such as OpenRouter, split their group by maker.
- Type to filter: every word must match the model's name, its id or its provider.
- <kbd>↑</kbd> <kbd>↓</kbd> move, <kbd>Enter</kbd> picks, <kbd>Esc</kbd> clears the filter and then closes.
- Click the star on a row, or press <kbd>⌘D</kbd>, to make it a favorite. Favorites are shared with the dropdowns in Settings.

The defaults for new chats and sub-agents are in **Settings › Agent defaults**. See [Settings](./settings).

## What the chat shows

Pi's replies stream in as Markdown, and your own messages show as Markdown too. Thinking is collapsed into a **Thought** row unless **Always expand thinking** is on. Tool calls show as rows (Reading, Edited, Searched, Ran a script…) that you can expand for their input and output; an edit has an **Open in diff** button.

Once Pi moves on, a turn's steps fold into one line, such as "Worked for 2m 14s · edited 3 files, ran 4 commands, read 6", with the lines it added and removed on the right. That covers thinking, tool calls and the notes Pi writes between them; your message and Pi's last reply stay as they are. Click the line to see the steps. While Pi works, the line says "Working for" and keeps counting, with Pi's latest note and the step running now below it. Approvals and anything else waiting for you never fold, nor do sub-agent, worktree, commit and merge cards, which split the steps. Failed steps show as "1 failed" in red on the line. A turn with fewer than three steps doesn't fold. Turn it off with **Collapse finished steps** in Settings.

### Code and syntax highlighting

Code blocks in the chat and the diff are highlighted for Astro, Bash, CSS, HTML, JavaScript, JSON, Markdown, Python, Rust, TOML, TSX, TypeScript and YAML, in the theme's Catppuccin colours. Other languages show as plain text.

### Tables

Markdown tables have a header row, hairline dividers and a copy button under the table, with **Copy as Markdown** and **Copy as CSV**. Columns whose values are all numbers, dates, sizes or durations are right-aligned once the reply has finished; an alignment marker in the Markdown (`:--:`) wins.


### Diagrams

A fenced ` ```mermaid ` block is drawn as its diagram, in the theme's colours: flowcharts, sequence, class, state and ER diagrams, Gantt charts, mind maps and more. **Diagram** and **Source** in the block's header switch between the drawing and the text, and the copy button copies the source. While Pi is still writing the block it shows "Rendering diagram…"; a diagram that can't be drawn shows its source with the reason. A diagram wider than the chat is scaled down to fit.

## Scrolling

With the chat focused, <kbd>Page Up</kbd> and <kbd>Page Down</kbd> scroll a page at a time, and <kbd>Ctrl+U</kbd> and <kbd>Ctrl+D</kbd> half a page, also while typing in the message box. Back at the end, the chat follows new output again.

While you're scrolled up, a small box above the message box says how much is below ("14 more below"), and in blue how much arrived since you scrolled away; with Pi still working it says so. Click it, or press <kbd>⌘↓</kbd> (<kbd>Ctrl+End</kbd>), to jump to the latest.

## Chat history

Each worktree's chat comes back when you restart Treehouse. Pi saves every session as a file in Treehouse's data folder, and Treehouse remembers which file belongs to which worktree (`sessions.json`, with its sub-agents). Your next message continues the same Pi session, so Pi has the whole conversation.

- `/new` starts a new session, which is the one that comes back from then on.
- Sub-agents of the current session come back too, under their worktree in the sidebar and as cards in the chat. You can keep talking to one, though its parent already has its report.
- What Treehouse adds to the chat comes back as well: the worktree's setup row, commit cards, notes such as "Pi restarted" and "Model changed", and MCP sign-ins. Treehouse saves them in Pi's session file, where the model doesn't see them.
- A chat with nothing but `!` commands isn't saved: Pi writes a session file once it has a reply. Until then, the setup row and notes wait with it.
