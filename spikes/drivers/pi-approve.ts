import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
	pi.on("tool_call", async (event, ctx) => {
		if (event.toolName !== "write" && event.toolName !== "edit" && event.toolName !== "bash") return undefined;
		const ok = await ctx.ui.confirm("Approve " + event.toolName + "?", JSON.stringify(event.input));
		return ok ? undefined : { block: true, reason: "denied by hub" };
	});
}
