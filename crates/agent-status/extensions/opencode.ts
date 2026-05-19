import { spawnSync } from "node:child_process";

/**
 * Bridges opencode lifecycle events to the `agent-status` CLI so opencode
 * sessions waiting on user input show up in tmux's status-right.
 *
 * Install: copy this file to `~/.config/opencode/plugins/opencode.ts`
 * (or `.opencode/plugins/opencode.ts` for per-project install).
 * Override the binary path with `AGENT_STATUS_BIN` if not at the default.
 * On Windows, `process.env.HOME` is undefined — set `AGENT_STATUS_BIN`
 * explicitly to an absolute path or the spawn will silently no-op.
 */
export const AgentStatusPlugin = async () => {
  return {
    event: async ({ event }: { event: any }) => {
      switch (event?.type) {
        case "session.idle":
          fire(
            event.properties?.sessionID,
            "set",
            "done",
            sessionIdleMessage(event),
          );
          return;
        case "permission.updated":
          fire(
            event.properties?.sessionID,
            "set",
            "notify",
            permissionMessage(event),
          );
          return;
        case "session.created":
        case "session.deleted":
          fire(event.properties?.info?.id, "clear");
          return;
      }
    },
  };
};

const BIN = process.env.AGENT_STATUS_BIN ?? "agent-status";

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(
  sessionId: string | undefined,
  action: Action,
  event?: SetEvent,
  message?: string,
): void {
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "opencode", event!]
      : ["clear", "--agent", "opencode"];

  const payload: Record<string, string> = { session_id: sessionId };
  if (message) payload.message = message;

  // spawnSync rather than spawn: in `opencode run` headless mode the parent
  // exits immediately after `session.idle` and an async child has no time to
  // execute. Blocking ~5-50ms here is invisible in practice and works in TUI
  // mode too. `error` (e.g. ENOENT when agent-status isn't installed) is
  // returned on the result object, not thrown — so we ignore it for
  // best-effort behavior.
  spawnSync(BIN, args, {
    input: JSON.stringify(payload),
    stdio: ["pipe", "ignore", "ignore"],
    timeout: 1000,
  });
}

/**
 * Best-effort extraction of a human-readable label from a `session.idle`
 * event. Probes commonly-used fields; returns `undefined` if nothing
 * suitable is present, in which case `message` is omitted from the payload.
 */
function sessionIdleMessage(event: any): string | undefined {
  const candidates: unknown[] = [
    event?.properties?.info?.title,
    event?.properties?.info?.summary,
    event?.properties?.title,
    event?.properties?.summary,
  ];
  for (const c of candidates) {
    if (typeof c === "string" && c.trim().length > 0) return c;
  }
  return undefined;
}

/**
 * Synthesize a short label for a `permission.updated` event. Falls back to
 * a generic "Permission requested" string when no specific action text is
 * reachable on the event.
 */
function permissionMessage(event: any): string {
  const action: unknown =
    event?.properties?.action ??
    event?.properties?.tool ??
    event?.properties?.title;
  if (typeof action === "string" && action.trim().length > 0) {
    return `Permission requested: ${action}`;
  }
  return "Permission requested";
}
