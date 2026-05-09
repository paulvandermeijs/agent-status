import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { spawn } from "node:child_process";
import { basename } from "node:path";

/**
 * Bridges pi-coding-agent lifecycle events to the `agent-status` CLI so pi
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.pi/agent/extensions/pi-coding-agent.ts`.
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 * On Windows, `process.env.HOME` is undefined — set `AGENT_STATUS_BIN`
 * explicitly to an absolute path or the spawn will silently no-op.
 */
export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) => fire(ctx, undefined, "clear"));
  pi.on("session_shutdown", async (_event, ctx) =>
    fire(ctx, undefined, "clear"),
  );
  pi.on("before_agent_start", async (_event, ctx) =>
    fire(ctx, undefined, "clear"),
  );
  pi.on("agent_end", async (event, ctx) =>
    fire(ctx, lastAgentMessage(event, ctx), "set", "done"),
  );
}

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(
  ctx: any,
  message: string | undefined,
  action: Action,
  event?: SetEvent,
): void {
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
  const payload: Record<string, string> = { session_id: sessionId };
  if (message) payload.message = message;
  child.stdin?.end(JSON.stringify(payload));
}

function sessionIdFromCtx(ctx: any): string | null {
  const file: string | null | undefined =
    ctx?.sessionManager?.getSessionFile?.();
  if (!file) return null;
  // pi session filenames are "<timestamp>_<uuid>.jsonl" — pull the UUID out.
  const match = basename(file, ".jsonl").match(/_([0-9a-f-]{36})$/i);
  return match ? match[1] : null;
}

/**
 * Best-effort extraction of the last assistant text from pi's `agent_end`
 * payload. The exact field name depends on pi's runtime shape; we probe a
 * handful of plausible spots and silently fall through when nothing is
 * present, in which case the JSON sent to `agent-status set` simply omits
 * the `message` field and the Rust side stores `message: None`.
 */
function lastAgentMessage(event: any, ctx: any): string | undefined {
  const candidates: unknown[] = [
    event?.response?.text,
    event?.message?.text,
    event?.lastMessage?.text,
    ctx?.lastAgentResponse?.text,
    ctx?.lastMessage?.text,
  ];
  for (const c of candidates) {
    if (typeof c === "string" && c.trim().length > 0) return c;
  }
  return undefined;
}
