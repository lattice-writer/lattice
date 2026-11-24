/**
 * Files dragged in from the Finder onto the Agent panel: the island hands
 * each phase of the drag to its owner, which tells the page
 * (`agent:file-drag`, `drops.rs`). Any file is an attachment, whatever its
 * format (lattice#1254), so any drag lights the panel while it is over it,
 * and on the drop the files' bytes, read by the owner from that drop's paths
 * alone, go into the composer over the embed bridge, as its "+" menu would
 * add them (`agent-composer-files.ts`). The owner reads at most the
 * composer's 20 and says how many it skipped, and refuses folders, empty
 * files and files over the limits; the refusal shows here in the interface
 * language.
 */
import { useEffect, useReducer } from "react";
import { invoke } from "@tauri-apps/api/core";
import { host } from "../shared/host";
import { showError, showNotice } from "../../../src/app/notify";
import { hostErrorMessage } from "../../../src/app-utils";
import {
  agentAttachmentsSkippedMessage,
  buildAgentComposerFilesMessage,
  type AgentComposerFilePayload,
} from "../../../src/agent/agent-composer-files";

type Phase = "enter" | "over" | "leave" | "drop";

/** What `read_agent_composer_files` answers in the native app (`read_agent_attachments`). */
export interface AgentAttachmentsRead {
  files: AgentComposerFilePayload[];
  skipped: number;
}

export interface AgentFileDragDeps {
  setActive: (active: boolean) => void;
  post: (message: object) => unknown;
  read: (paths: string[]) => Promise<AgentAttachmentsRead>;
  fail: (message: string) => void;
  skipped: (attached: number, skipped: number) => void;
}

function parseDrag(payload: unknown): { phase: Phase; paths: string[] } {
  const { phase, paths } = (payload ?? {}) as { phase?: unknown; paths?: unknown };
  return {
    phase: phase === "enter" || phase === "over" || phase === "drop" ? phase : "leave",
    paths: Array.isArray(paths) ? paths.filter((path): path is string => typeof path === "string") : [],
  };
}

/** The `agent:file-drag` listener: it lights the panel, and reads and posts a drop's files. */
export function agentFileDragListener(deps: AgentFileDragDeps): (payload: unknown) => Promise<void> {
  // WebKit names the files on entering and dropping only, as Tauri does.
  let dragged: string[] = [];
  return async (payload) => {
    const { phase, paths } = parseDrag(payload);
    if (phase === "enter") dragged = paths;
    if (phase === "over" || phase === "enter") {
      deps.setActive(dragged.length > 0);
      return;
    }
    deps.setActive(false);
    dragged = [];
    if (phase !== "drop" || !paths.length) return;
    try {
      const { files, skipped } = await deps.read(paths);
      deps.post(buildAgentComposerFilesMessage(files));
      if (skipped > 0) deps.skipped(files.length, skipped);
    } catch (error) {
      deps.fail(hostErrorMessage(error));
    }
  };
}

/** Whether a drag of files is over the panel; `post` relays a dropped file set to its frame. */
export function useAgentFileDrops(enabled: boolean, post: (message: object) => unknown): boolean {
  const [active, setActive] = useReducer((_: boolean, next: boolean) => next, false);
  useEffect(() => {
    if (!enabled) return;
    const listen = agentFileDragListener({
      setActive,
      post,
      read: (paths) => invoke<AgentAttachmentsRead>("read_agent_composer_files", { paths }),
      fail: showError,
      skipped: (attached, skipped) =>
        showNotice(agentAttachmentsSkippedMessage(attached, skipped)),
    });
    const stop = host().on("agent:file-drag", (payload) => void listen(payload));
    return () => {
      stop();
      setActive(false);
    };
  }, [enabled, post]);
  return active;
}
