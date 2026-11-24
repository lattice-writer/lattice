import { t } from "@lingui/core/macro";

const LATTICE_COMPOSER_FILES = "lattice:composer-files";

/** Mirrors the caps the embedded panel enforces when validating the message. */
const MAX_AGENT_COMPOSER_FILES = 20;

/** Shape returned by the `read_agent_composer_files` Tauri command. */
export interface AgentComposerFilePayload {
  name: string;
  mimeType: string;
  bytesBase64: string;
}

interface AgentComposerFileEntry {
  name: string;
  mimeType: string;
  bytes: ArrayBuffer;
}

export interface AgentComposerFilesMessage {
  type: typeof LATTICE_COMPOSER_FILES;
  version: 1;
  files: AgentComposerFileEntry[];
}

export function buildAgentComposerFilesMessage(
  payloads: readonly AgentComposerFilePayload[],
): AgentComposerFilesMessage {
  return {
    type: LATTICE_COMPOSER_FILES,
    version: 1,
    files: payloads.slice(0, MAX_AGENT_COMPOSER_FILES).map((payload) => ({
      name: payload.name,
      mimeType: payload.mimeType,
      bytes: Uint8Array.from(atob(payload.bytesBase64), (character) => character.charCodeAt(0)).buffer,
    })),
  };
}

/**
 * The native app's note that a drop held more files than the composer takes
 * (`read_agent_attachments`). Its one caller is the Agent island
 * (`gpui/islands/agent/drops.ts`), which knip does not read.
 * @public
 */
export function agentAttachmentsSkippedMessage(attached: number, skipped: number): string {
  return t`The Agent takes ${attached} files at a time, so ${skipped} more weren’t attached.`;
}
