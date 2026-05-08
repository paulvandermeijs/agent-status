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
          fire(event.properties?.sessionID, "set", "done");
          return;
        case "permission.updated":
          fire(event.properties?.sessionID, "set", "notify");
          return;
        case "session.created":
        case "session.deleted":
          fire(event.properties?.info?.id, "clear");
          return;
      }
    },
  };
};

const BIN =
  process.env.AGENT_STATUS_BIN ?? `${process.env.HOME}/.claude/bin/agent-status`;

type Action = "set" | "clear";
type SetEvent = "notify" | "done";

function fire(
  sessionId: string | undefined,
  action: Action,
  event?: SetEvent,
): void {
  if (!sessionId) return;

  const args =
    action === "set"
      ? ["set", "--agent", "opencode", event!]
      : ["clear", "--agent", "opencode"];

  // spawnSync rather than spawn: in `opencode run` headless mode the parent
  // exits immediately after `session.idle` and an async child has no time to
  // execute. Blocking ~5-50ms here is invisible in practice and works in TUI
  // mode too. `error` (e.g. ENOENT when agent-status isn't installed) is
  // returned on the result object, not thrown — so we ignore it for
  // best-effort behavior.
  spawnSync(BIN, args, {
    input: JSON.stringify({ session_id: sessionId }),
    stdio: ["pipe", "ignore", "ignore"],
    timeout: 1000,
  });
}
