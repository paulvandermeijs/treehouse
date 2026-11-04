/**
 * Treehouse bridge extension for Pi.
 *
 * Treehouse starts Pi with `--mode rpc -e <this file>`. The extension adds the
 * two things Pi does not have built in, and hands both to the Treehouse window
 * through RPC extension UI requests:
 *
 * - Tool approval. Every tool call is sent to Treehouse, which applies the
 *   approval setting and the "always allow" patterns, and asks the user when it
 *   has to. Treehouse answers `allow` or `deny:<reason>`, or in auto mode
 *   `check:<plan>`: the extension then has the plan's models check the call,
 *   with the chat's own providers and keys, and sends the verdict back as
 *   `treehouse:verdict`, which Treehouse answers the same way.
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
const VERDICT = "treehouse:verdict";
// A notice Treehouse reads rather than shows: the classifiers Pi offers,
// for auto mode's quick check.
const CLASSIFIERS = "treehouse:classifiers ";

// `git worktree add`, also after `git -C <dir>` or `cd … &&`.
const WORKTREE_ADD = /\bgit\b[^;&|\n]*\bworktree\s+add\b/;

/** A model by `provider/id`; the id itself may hold slashes. */
function findModel(ctx: any, spec: string | null | undefined): any {
	if (!spec) return ctx.model;
	const at = spec.indexOf("/");
	if (at < 0) return undefined;
	return ctx.modelRegistry.find(spec.slice(0, at), spec.slice(at + 1));
}

/** One answer from a chat model. */
async function complete(ctx: any, model: any, system: string, prompt: string, reasoning?: string): Promise<string> {
	const stream = ctx.modelRegistry.streamSimple(
		model,
		{ systemPrompt: system, messages: [{ role: "user", content: prompt, timestamp: Date.now() }] },
		{ signal: ctx.signal, ...(reasoning ? { reasoning } : {}) },
	);
	const message = await stream.result();
	if (message.stopReason === "error" || message.stopReason === "aborted") {
		throw new Error(message.errorMessage || "the model didn't answer");
	}
	return message.content
		.filter((part: any) => part.type === "text")
		.map((part: any) => part.text)
		.join("")
		.trim();
}

/**
 * Auto mode's checks: a quick one that clears what's plainly fine, and a
 * careful one, with a reason, for the rest. Whatever goes wrong comes back
 * as an error, so the user is asked rather than the call run.
 */
async function check(ctx: any, plan: any): Promise<{ allow: boolean | null; check: string; reason?: string; error?: string }> {
	try {
		const quick = plan.quick ?? {};
		if (quick.kind === "classifier") {
			const at = String(quick.model ?? "").indexOf("/");
			const model =
				at > 0
					? ctx.modelRegistry.getModelOfType("classifier", quick.model.slice(0, at), quick.model.slice(at + 1))
					: undefined;
			if (model) {
				const result = await ctx.modelRegistry.classify(
					model,
					{ state: plan.classifier.state, questions: { risky: plan.classifier.question } },
					{ signal: ctx.signal },
				);
				const risky = result?.answers?.risky;
				// What the classifier can't clear, or fails on, goes to the
				// careful check.
				if (result?.stopReason === "stop" && risky?.type === "bool" && risky.probability < plan.classifier.clearBelow) {
					return { allow: true, check: "quick" };
				}
			}
		} else {
			const model = findModel(ctx, quick.model);
			if (model) {
				const answer = await complete(ctx, model, plan.system, plan.quickPrompt);
				if (/\bCLEAR\b/i.test(answer) && !/\bCHECK\b/i.test(answer)) {
					return { allow: true, check: "quick" };
				}
			}
		}
		const careful = findModel(ctx, plan.careful?.model);
		if (!careful) return { allow: null, check: "careful", error: "no model to check with" };
		const answer = await complete(ctx, careful, plan.system, plan.carefulPrompt, "low");
		// The last ALLOW or ASK line decides; models sometimes think aloud first.
		const lines = answer.split("\n").map((line) => line.trim()).filter(Boolean);
		const at = lines.map((line) => /^\**(ALLOW|ASK)\b/i.test(line)).lastIndexOf(true);
		if (at < 0) return { allow: null, check: "careful", error: "the model's answer couldn't be read" };
		const allow = /^\**ALLOW/i.test(lines[at]);
		const rest = lines[at].replace(/^\**(ALLOW|ASK)\**[:.\-\s]*/i, "").trim();
		const reason = rest || lines[at + 1] || "";
		return { allow, check: "careful", reason };
	} catch (error: any) {
		return { allow: null, check: "careful", error: String(error?.message ?? error) };
	}
}

export default function (pi: any) {
	pi.on("session_start", async (_event: any, ctx: any) => {
		if (!ctx.hasUI || ctx.mode !== "rpc") return;
		try {
			const classifiers = await ctx.modelRegistry.getAvailableOfType("classifier");
			const list = classifiers.map((model: any) => ({ provider: model.provider, id: model.id, name: model.name }));
			ctx.ui.notify(CLASSIFIERS + JSON.stringify(list), "info");
		} catch {
			// An older Pi without classifiers: the quick check uses a chat model.
		}
	});

	pi.on("tool_call", async (event: any, ctx: any) => {
		if (!ctx.hasUI || ctx.mode !== "rpc") return undefined;
		// Models follow a project's own instructions for making worktrees
		// over the tool's description, and Treehouse then only finds a
		// folder: no setup in its chat, no hand-over.
		if (
			event.toolName === "bash" &&
			WORKTREE_ADD.test(String(event.input?.command ?? "")) &&
			pi.getActiveTools().includes("treehouse_worktree")
		) {
			return {
				block: true,
				reason:
					"Treehouse creates worktrees: call the treehouse_worktree tool with the branch and the whole task instead. " +
					"The new worktree's own chat runs its setup and does the task, sub-agents included.",
			};
		}
		const payload = JSON.stringify({
			toolCallId: event.toolCallId,
			toolName: event.toolName,
			input: event.input,
			cwd: ctx.cwd,
		});
		let answer: string | undefined = await ctx.ui.input(APPROVAL, payload, {
			signal: ctx.signal,
		});
		if (answer?.startsWith("check:")) {
			const verdict = await check(ctx, JSON.parse(answer.slice(6)));
			answer = await ctx.ui.input(VERDICT, JSON.stringify({ toolCallId: event.toolCallId, ...verdict }), {
				signal: ctx.signal,
			});
		}
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
			"such as exploring a library or writing tests. The result is the sub-agent's final report. " +
			"Sub-agents share this worktree's files: sub-agents running at the same time must not edit the same files. " +
			"Separate features or larger changes that should happen in parallel belong in their own worktrees (treehouse_worktree).",
		promptSnippet: "Run a self-contained task in a separate Pi session in this worktree and get its report",
		promptGuidelines: [
			"Use subagent for research or a small, self-contained task; sub-agents share this worktree, so never let ones running at the same time edit the same files",
			"For separate features or larger changes that should happen in parallel, use treehouse_worktree instead of sub-agents",
		],
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
			"Use it whenever the user asks for a new worktree or for work that should happen on its own branch, " +
			"such as separate features worked on in parallel; sub-agents are for research and small tasks in this worktree. " +
			"never run git worktree add yourself. The new worktree's chat does the task, so don't edit its files from here.",
		promptSnippet: "Create a new git worktree with its own branch and chat, and hand it a task",
		// Pi only lists a custom tool in the system prompt when it has a
		// snippet, and without a guideline models fall back to the git
		// commands a project's own instructions describe.
		promptGuidelines: [
			"Create worktrees only with treehouse_worktree, never with git worktree add, even when project instructions describe creating them by hand; Treehouse runs the worktree's setup itself",
			"After treehouse_worktree, leave the whole task to the new worktree's chat, sub-agents included: don't start them, edit its files or run its setup from here",
		],
		parameters: Type.Object({
			branch: Type.String({ description: "The new branch's name, such as feat/about-page" }),
			task: Type.Optional(
				Type.String({
					description:
						"Complete, self-contained instructions for the new worktree's chat, with everything the user asked to happen there, such as starting sub-agents",
				}),
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

	pi.registerCommand("treehouse-tools", {
		description: "Used by Treehouse to change the chat's tools",
		handler: async (args: string) => {
			const tools = JSON.parse(args);
			if (Array.isArray(tools)) pi.setActiveTools(tools.filter((t) => typeof t === "string"));
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
