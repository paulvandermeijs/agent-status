import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { spawn } from "node:child_process";
import { basename } from "node:path";

/**
 * Bridges pi-coding-agent lifecycle events to the `agent-status` CLI so pi
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.pi/agent/extensions/pi-coding-agent.ts`.
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 */
export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => fire(ctx, "clear"));
  pi.on("session_shutdown", async (_event, ctx) => fire(ctx, "clear"));
  pi.on("before_agent_start", async (_event, ctx) => fire(ctx, "clear"));
  pi.on("agent_end", async (_event, ctx) => fire(ctx, "set", "done"));
}

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(ctx: any, action: Action, event?: SetEvent): void {
  const sessionId = sessionIdFromCtx(ctx);
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "pi-coding-agent", event!]
      : ["clear", "--agent", "pi-coding-agent"];

  const child = spawn(BIN, args, {
    stdio: ["pipe", "ignore", "ignore"],
  });
  child.on("error", () => {
    // best-effort: agent-status may not be installed; never crash pi
  });
  child.stdin?.end(JSON.stringify({ session_id: sessionId }));
}

function sessionIdFromCtx(ctx: any): string | null {
  const file: string | null | undefined = ctx?.sessionManager?.getSessionFile?.();
  if (!file) return null;
  // pi session filenames are "<timestamp>_<uuid>.jsonl" — pull the UUID out.
  const match = basename(file, ".jsonl").match(/_([0-9a-f-]{36})$/i);
  return match ? match[1] : null;
}
