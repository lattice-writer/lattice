/**
 * The Agent working in a folder above the project (lattice#1293): the native
 * app can give the Agent `ProjectA/` while the project, its history and its
 * builds stay `ProjectA/Manuscript/`. The Synara frame then works in
 * `ProjectA/`, so its paths are relative to that folder, while every host
 * tool, file command and snapshot here speaks of the project.
 *
 * The frame's messages are translated at the frame's edge, both ways, and
 * nowhere else: `workspaceRoot` is swapped between the two folders, and a
 * workspace-relative path under one of the keys below gains or loses the
 * project's place in the Agent's folder (`Manuscript/`). A path the Agent
 * names outside the project is given a `..` part, which every project path
 * check refuses, so it reaches no project file. Absolute paths and URLs pass
 * as they are. Without a folder above the project nothing is translated.
 */

export type AgentFolderView = {
  projectRoot: string;
  agentRoot: string;
  /** The project's folder inside the Agent's, with forward slashes and a trailing one. */
  prefix: string;
};

const PATH_KEYS = new Set(["path", "filePath", "document", "pagePath", "deckPath", "pdfPath", "activeFile"]);
const PATH_LIST_KEYS = new Set(["paths"]);
const ROOT_KEY = "workspaceRoot";
const MAX_DEPTH = 8;

const trimmed = (root: string) => root.replace(/[\\/]+$/, "");

// A path is one line without markup: `synara:expand-visual` carries a whole
// HTML page under `document`, the figure tool a path under the same key.
function isRelative(path: string): boolean {
  return path.length > 0
    && !/[\n\r<>]/.test(path)
    && !path.startsWith("/")
    && !path.startsWith("\\")
    && !/^[A-Za-z]:[\\/]/.test(path)
    && !/^[A-Za-z][A-Za-z0-9+.-]*:/.test(path);
}

/** The view of the project from `agentRoot`, or null when the Agent works in the project itself. */
export function agentFolderView(projectRoot: string | null | undefined, agentRoot: string | null | undefined): AgentFolderView | null {
  if (!projectRoot || !agentRoot) return null;
  const project = trimmed(projectRoot);
  const agent = trimmed(agentRoot);
  if (!agent || project.length <= agent.length + 1 || !project.startsWith(agent) || !/[\\/]/.test(project[agent.length])) return null;
  return { projectRoot: project, agentRoot: agent, prefix: `${project.slice(agent.length + 1).replace(/\\/g, "/")}/` };
}

function rewrite(value: unknown, key: string | null, depth: number, path: (value: string) => string, root: (value: string) => string): unknown {
  if (typeof value === "string") {
    if (key === ROOT_KEY) return root(value);
    return key !== null && PATH_KEYS.has(key) && isRelative(value) ? path(value) : value;
  }
  if (value === null || typeof value !== "object" || depth >= MAX_DEPTH) return value;
  if (Array.isArray(value)) {
    const list = key !== null && PATH_LIST_KEYS.has(key);
    return value.map((item) => (list && typeof item === "string" && isRelative(item)
      ? path(item)
      : rewrite(item, null, depth + 1, path, root)));
  }
  return Object.fromEntries(Object.entries(value).map(([name, item]) => [name, rewrite(item, name, depth + 1, path, root)]));
}

/** A message for the frame: the project's root and paths as the Agent's folder sees them. */
export function toAgentFolder<Message>(view: AgentFolderView | null, message: Message): Message {
  if (!view) return message;
  return rewrite(
    message, null, 0,
    (path) => view.prefix + path.replace(/^\.\//, ""),
    (root) => (trimmed(root) === view.projectRoot ? view.agentRoot : root),
  ) as Message;
}

/** A message from the frame: its folder's root and paths as the project's. */
export function fromAgentFolder(view: AgentFolderView | null, data: unknown): unknown {
  if (!view) return data;
  return rewrite(
    data, null, 0,
    (path) => {
      const relative = path.replace(/\\/g, "/").replace(/^\.\//, "");
      return relative.startsWith(view.prefix) ? relative.slice(view.prefix.length) : `../${relative}`;
    },
    (root) => (trimmed(root) === view.agentRoot ? view.projectRoot : root),
  );
}
