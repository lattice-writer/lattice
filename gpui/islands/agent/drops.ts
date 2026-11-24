/**
 * Files dragged in from the Finder onto the Agent panel: the island hands
 * each phase of the drag to its owner, which tells the page
 * (`agent:file-drag`, `drops.rs`). Any file is an attachment, whatever its
 * format (lattice#1254), so any drag lights the panel while it is over it,
 * and on the drop the files' bytes, read by the owner from that drop's paths
 * alone, go into the composer over the embed bridge, as its "+" menu would
 * add them (`agent-composer-files.ts`). The owner refuses folders and empty
 * files, and the error shows here.
 */
import { useEffect, useReducer } from "react";
import { invoke } from "@tauri-apps/api/core";
import { host } from "../shared/host";
import { showError } from "../../../src/app/notify";
import { toMessage } from "../../../src/app-utils";
import { buildAgentComposerFilesMessage, type AgentComposerFilePayload } from "../../../src/agent/agent-composer-files";

type Phase = "enter" | "over" | "leave" | "drop";

function parseDrag(payload: unknown): { phase: Phase; paths: string[] } {
  const { phase, paths } = (payload ?? {}) as { phase?: unknown; paths?: unknown };
  return {
    phase: phase === "enter" || phase === "over" || phase === "drop" ? phase : "leave",
    paths: Array.isArray(paths) ? paths.filter((path): path is string => typeof path === "string") : [],
  };
}

/** The `agent:file-drag` listener: `setActive` lights the panel, and a drop's files are read and posted. */
export function agentFileDragListener(
  setActive: (active: boolean) => void,
  post: (message: object) => unknown,
  read: (paths: string[]) => Promise<AgentComposerFilePayload[]> = (paths) =>
    invoke<AgentComposerFilePayload[]>("read_agent_composer_files", { paths }),
  fail: (message: string) => void = showError,
): (payload: unknown) => Promise<void> {
  // WebKit names the files on entering and dropping only, as Tauri does.
  let dragged: string[] = [];
  return async (payload) => {
    const { phase, paths } = parseDrag(payload);
    if (phase === "enter") dragged = paths;
    if (phase === "over" || phase === "enter") {
      setActive(dragged.length > 0);
      return;
    }
    setActive(false);
    dragged = [];
    if (phase !== "drop" || !paths.length) return;
    try {
      post(buildAgentComposerFilesMessage(await read(paths)));
    } catch (error) {
      fail(toMessage(error));
    }
  };
}

/** Whether a drag of files is over the panel; `post` relays a dropped file set to its frame. */
export function useAgentFileDrops(enabled: boolean, post: (message: object) => unknown): boolean {
  const [active, setActive] = useReducer((_: boolean, next: boolean) => next, false);
  useEffect(() => {
    if (!enabled) return;
    const listen = agentFileDragListener(setActive, post);
    const stop = host().on("agent:file-drag", (payload) => void listen(payload));
    return () => {
      stop();
      setActive(false);
    };
  }, [enabled, post]);
  return active;
}
