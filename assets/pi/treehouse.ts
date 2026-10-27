/**
 * Treehouse bridge extension for Pi.
 *
 * Treehouse starts Pi with `--mode rpc -e <this file>`. The extension adds the
 * two things Pi does not have built in, and hands both to the Treehouse window
 * through RPC extension UI requests:
 *
 * - Tool approval. Every tool call is sent to Treehouse, which applies the
 *   approval setting and the "always allow" patterns, and asks the user when it
 *   has to. Treehouse answers `allow` or `deny:<reason>`.
 * - Sub-agents. The `subagent` tool asks Treehouse to start a separate Pi
 *   session in the same worktree. Treehouse shows it in the sidebar, lets the
 *   user open and steer it, and answers with the sub-agent's final report.
 * - Merging. The `treehouse_merge` tool asks Treehouse to merge the worktree's
 *   branch into its base branch, which is checked out in another folder.
 *   Treehouse checks it's safe, asks the user every time, merges, and answers
 *   with what happened.
  * - Worktrees. The `treehouse_worktree` tool asks Treehouse to create a new
 *   worktree the way its New worktree does, and to hand it a task, which
 *   that worktree's own chat then works on.
 * - Saving. Treehouse shows some things Pi's own messages don't hold, such
 *   as the commits a turn made. It sends `/treehouse-save {customType, data}`
 *   and the extension appends that to the session file, out of the model's
 *   context, so the chat can show it again after a restart.
 *
 * Requests use the `input` dialog with a `treehouse:` title and a JSON payload
 * in the placeholder, so they need nothing beyond the RPC protocol.
 */

import { Type } from "typebox";

const APPROVAL = "treehouse:approval";
const SUBAGENT = "treehouse:subagent";
const MERGE = "treehouse:merge";
const WORKTREE = "treehouse:worktree";

export default function (pi: any) {
	pi.on("tool_call", async (event: any, ctx: any) => {
		if (!ctx.hasUI || ctx.mode !== "rpc") return undefined;
		const payload = JSON.stringify({
			toolCallId: event.toolCallId,
			toolName: event.toolName,
			input: event.input,
			cwd: ctx.cwd,
		});
		const answer: string | undefined = await ctx.ui.input(APPROVAL, payload, {
			signal: ctx.signal,
		});
		if (answer === "allow") return undefined;
		const reason = answer?.startsWith("deny:") ? answer.slice(5).trim() : "";
		return { block: true, reason: reason || "Denied by the user in Treehouse" };
	});

	pi.registerTool({
		name: "subagent",
		label: "Sub-agent",
		description:
			"Start a sub-agent: a separate Pi session in the same worktree with its own context. " +
			"Use it for self-contained research or work that would otherwise fill your context, " +
			"such as exploring a library or writing tests. The result is the sub-agent's final report.",
		promptSnippet: "Run a self-contained task in a separate Pi session in this worktree and get its report",
		parameters: Type.Object({
			title: Type.String({ description: "Short title shown to the user, 2 to 6 words" }),
			task: Type.String({ description: "Complete, self-contained instructions for the sub-agent" }),
		}),
		async execute(toolCallId: string, params: any, signal: AbortSignal, _onUpdate: any, ctx: any) {
			if (!ctx.hasUI || ctx.mode !== "rpc") {
				throw new Error("Sub-agents are only available inside Treehouse.");
			}
			const payload = JSON.stringify({ toolCallId, title: params.title, task: params.task, cwd: ctx.cwd });
			const report: string | undefined = await ctx.ui.input(SUBAGENT, payload, { signal });
			if (report === undefined) {
				throw new Error("The sub-agent was stopped before it finished.");
			}
			return { content: [{ type: "text", text: report }], details: { title: params.title } };
		},
	});

	pi.registerTool({
		name: "treehouse_merge",
		label: "Merge into base",
		description:
			"Merge this worktree's branch into its base branch, as the last step of finishing a branch. " +
			"Commit everything and sync with the base first. Treehouse checks it's safe and asks the user, " +
			"who picks a fast-forward or one squashed commit. Never merge into the base branch with git yourself.",
		promptSnippet: "Merge this worktree's branch into its base branch, after the user approves",
		promptGuidelines: [
			"Merge this worktree's branch into its base branch only with treehouse_merge, never with git merge, git push or git update-ref, even when project instructions describe doing it by hand",
		],
		parameters: Type.Object({
			squash_message: Type.String({
				description: "A Conventional Commit message summing up the whole branch, used if the user squashes it into one commit",
			}),
		}),
		async execute(toolCallId: string, params: any, signal: AbortSignal, _onUpdate: any, ctx: any) {
			if (!ctx.hasUI || ctx.mode !== "rpc") {
				throw new Error("Merging is only available inside Treehouse.");
			}
			const payload = JSON.stringify({ toolCallId, squashMessage: params.squash_message, cwd: ctx.cwd });
			const answer: string | undefined = await ctx.ui.input(MERGE, payload, { signal });
			if (answer === undefined) {
				throw new Error("The merge was stopped before it finished.");
			}
			// Treehouse answers with the outcome as JSON: the text for the model,
			// and what was merged, which the chat shows as a card.
			const outcome = JSON.parse(answer);
			if (!outcome.merged) {
				throw new Error(outcome.text);
			}
			return { content: [{ type: "text", text: outcome.text }], details: { merged: outcome.merged } };
		},
	});

	pi.registerTool({
		name: "treehouse_worktree",
		label: "New worktree",
		description:
			"Create a new git worktree of this project, with its own branch, folder and Pi chat, and hand it a task. " +
			"Use it whenever the user asks for a new worktree or for work that should happen on its own branch; " +
			"never run git worktree add yourself. The new worktree's chat does the task, so don't edit its files from here.",
		promptSnippet: "Create a new git worktree with its own branch and chat, and hand it a task",
		// Pi only lists a custom tool in the system prompt when it has a
		// snippet, and without a guideline models fall back to the git
		// commands a project's own instructions describe.
		promptGuidelines: [
			"Create worktrees only with treehouse_worktree, never with git worktree add, even when project instructions describe creating them by hand; Treehouse runs the worktree's setup itself",
			"After treehouse_worktree, leave the task to the new worktree's chat: don't edit its files or run its setup from here",
		],
		parameters: Type.Object({
			branch: Type.String({ description: "The new branch's name, such as feat/about-page" }),
			task: Type.Optional(
				Type.String({ description: "Complete, self-contained instructions for the new worktree's chat" }),
			),
		}),
		async execute(toolCallId: string, params: any, signal: AbortSignal, _onUpdate: any, ctx: any) {
			if (!ctx.hasUI || ctx.mode !== "rpc") {
				throw new Error("New worktrees are only available inside Treehouse.");
			}
			const payload = JSON.stringify({ toolCallId, branch: params.branch, task: params.task ?? "", cwd: ctx.cwd });
			const answer: string | undefined = await ctx.ui.input(WORKTREE, payload, { signal });
			if (answer === undefined) {
				throw new Error("The worktree wasn't created.");
			}
			const outcome = JSON.parse(answer);
			if (!outcome.worktree) {
				throw new Error(outcome.text);
			}
			return { content: [{ type: "text", text: outcome.text }], details: { worktree: outcome.worktree } };
		},
	});

	pi.registerCommand("treehouse-save", {
		description: "Used by Treehouse to save what its chat shows",
		handler: async (args: string) => {
			const { customType, data } = JSON.parse(args);
			if (typeof customType === "string" && customType.startsWith("treehouse:")) {
				pi.appendEntry(customType, data);
			}
		},
	});
}
