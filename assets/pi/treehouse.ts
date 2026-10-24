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
 *
 * Requests use the `input` dialog with a `treehouse:` title and a JSON payload
 * in the placeholder, so they need nothing beyond the RPC protocol.
 */

import { Type } from "typebox";

const APPROVAL = "treehouse:approval";
const SUBAGENT = "treehouse:subagent";

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
}
