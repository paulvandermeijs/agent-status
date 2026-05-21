import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";
import { spawn } from "node:child_process";
import { basename } from "node:path";

/**
 * Bridges pi-coding-agent lifecycle events to the `agent-status` CLI so pi
 * sessions show up in tmux's status-right and the agent-switcher popup.
 *
 * Install: copy this file to `~/.pi/agent/extensions/pi-coding-agent.ts`.
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 * On Windows, `process.env.HOME` is undefined — set `AGENT_STATUS_BIN`
 * explicitly to an absolute path or the spawn will silently no-op.
 *
 * Event mapping (mirrors the Claude Code hook contract):
 *   session_start        → set idle    (placeholder so the switcher sees the
 *                                       session from the moment pi launches)
 *   before_agent_start   → set working (activity = first line of user prompt)
 *   tool_execution_start → set working (activity = "Running: …" / "Reading X")
 *   agent_end            → set done    (activity = assistant's last text)
 *   session_shutdown     → clear       (the only event that removes the row)
 */
export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (_event, ctx) =>
    fire(ctx, undefined, "set", "idle"),
  );
  pi.on("session_shutdown", async (_event, ctx) =>
    fire(ctx, undefined, "clear"),
  );
  pi.on("before_agent_start", async (event, ctx) =>
    fire(ctx, summarizePrompt(event.prompt), "set", "working"),
  );
  pi.on("tool_execution_start", async (event, ctx) =>
    fire(ctx, formatToolActivity(event.toolName, event.args), "set", "working"),
  );
  pi.on("agent_end", async (event, ctx) =>
    fire(ctx, lastAgentMessage(event), "set", "done"),
  );
}

const BIN = process.env.AGENT_STATUS_BIN ?? "agent-status";

type Action = "set" | "clear";
type SetEvent = "notify" | "done" | "working" | "idle";

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
 * First non-empty line of a user prompt, capped at 80 chars. Used as the
 * Activity column placeholder between `before_agent_start` and the first
 * `tool_execution_start`, so the switcher shows what pi is working on even
 * before any tool runs.
 */
function summarizePrompt(prompt: unknown): string | undefined {
  if (typeof prompt !== "string") return undefined;
  const firstLine = prompt.trim().split("\n", 1)[0]?.trim();
  if (!firstLine) return undefined;
  const MAX = 80;
  return firstLine.length > MAX
    ? `${firstLine.slice(0, MAX - 1).trimEnd()}…`
    : firstLine;
}

/**
 * One-line activity string built from a pi tool execution. Mirrors
 * `format_pre_tool_use_activity` in `agents/claude_code.rs` but keyed by pi's
 * lowercase built-in tool names (bash/read/edit/write/grep/find/ls). Custom
 * tools fall through to a generic "Using <tool>" label.
 */
function formatToolActivity(
  toolName: unknown,
  args: unknown,
): string | undefined {
  if (typeof toolName !== "string" || toolName.length === 0) return undefined;
  const a = (args && typeof args === "object" ? args : {}) as Record<
    string,
    unknown
  >;
  const str = (v: unknown): string | undefined =>
    typeof v === "string" && v.length > 0 ? v : undefined;
  const pathBase = (v: unknown): string | undefined => {
    const s = str(v);
    return s ? basename(s) || s : undefined;
  };
  switch (toolName) {
    case "bash": {
      const cmd = str(a.command);
      if (!cmd) return "Running command";
      return `Running: ${cmd.replace(/\s+/g, " ").trim()}`;
    }
    case "read": {
      const p = pathBase(a.path);
      return p ? `Reading ${p}` : "Reading file";
    }
    case "edit": {
      const p = pathBase(a.path);
      return p ? `Editing ${p}` : "Editing file";
    }
    case "write": {
      const p = pathBase(a.path);
      return p ? `Writing ${p}` : "Writing file";
    }
    case "grep": {
      const pat = str(a.pattern);
      return pat ? `Searching for: ${pat}` : "Searching";
    }
    case "find": {
      const pat = str(a.pattern);
      return pat ? `Finding: ${pat}` : "Finding files";
    }
    case "ls": {
      const p = pathBase(a.path);
      return p ? `Listing ${p}` : "Listing directory";
    }
    default:
      return `Using ${toolName}`;
  }
}

/**
 * Extract the assistant's last text from an `agent_end` event. Walks
 * `event.messages` from end to start, picks the most recent `role:
 * "assistant"` entry, and concatenates its `type: "text"` content blocks.
 * Returns undefined when the turn produced no text (tool-only, aborted, or
 * errored) — the Rust side then stores `message: None` and the switcher's
 * Activity column stays blank for that row.
 */
function lastAgentMessage(event: unknown): string | undefined {
  const messages: unknown[] = Array.isArray((event as any)?.messages)
    ? (event as any).messages
    : [];
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i] as { role?: unknown; content?: unknown };
    if (m?.role !== "assistant") continue;
    return collectAssistantText(m.content);
  }
  return undefined;
}

function collectAssistantText(content: unknown): string | undefined {
  if (typeof content === "string") {
    const trimmed = content.trim();
    return trimmed.length > 0 ? trimmed.replace(/\s+/g, " ") : undefined;
  }
  if (!Array.isArray(content)) return undefined;
  const parts: string[] = [];
  for (const block of content) {
    const b = block as { type?: unknown; text?: unknown };
    if (b?.type === "text" && typeof b.text === "string" && b.text.length > 0) {
      parts.push(b.text);
    }
  }
  const joined = parts.join(" ").replace(/\s+/g, " ").trim();
  if (!joined) return undefined;
  const MAX = 200;
  return joined.length > MAX
    ? `${joined.slice(0, MAX - 1).trimEnd()}…`
    : joined;
}
