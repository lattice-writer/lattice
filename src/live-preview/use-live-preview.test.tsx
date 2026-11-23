import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectSnapshot } from "../app-types";
import { persistLivePreviewEnabled } from "../settings/app-settings";
import { MAX_RESTARTS } from "./live-preview-session";
import { useLivePreview, type LivePreviewInstall } from "./use-live-preview";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: (message: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const started = vi.hoisted(() => [] as string[]);
/** Each session made, so a test can have its rtex exit. */
const sessions = vi.hoisted(() => [] as Array<{ exit: (signal: number) => void; report: () => void; dispose: () => void }>);
vi.mock("./live-preview-chunk", async () => ({
  watchRestarts: (await vi.importActual<typeof import("./live-preview-session")>("./live-preview-session")).watchRestarts,
  LivePreviewSession: class {
    private snapshot = { phase: "live", layoutPending: false, detail: null as string | null, exit: null as unknown, rebuilding: false };
    private readonly listeners = new Set<() => void>();
    constructor(root: string, main: string) {
      started.push(main === "main.tex" ? root : `${root}:${main}`);
      sessions.push(this);
    }
    start = vi.fn(async () => {});
    dispose = vi.fn();
    noteText = vi.fn();
    noteFilesChanged = vi.fn();
    subscribe = (listener: () => void) => {
      this.listeners.add(listener);
      return () => this.listeners.delete(listener);
    };
    getSnapshot = () => this.snapshot;
    exit(signal: number) {
      this.snapshot = { ...this.snapshot, phase: "exited", detail: "rtex: out of memory", exit: { code: null, signal } };
      for (const listener of [...this.listeners]) listener();
    }
    /** Another snapshot change after the exit, as a build still reporting to the session makes. */
    report() {
      this.snapshot = { ...this.snapshot, rebuilding: !this.snapshot.rebuilding };
      for (const listener of [...this.listeners]) listener();
    }
  },
}));

const READY: LivePreviewInstall = { supported: true, lualatex: true, installed: false, version: "0.0.6" };
let install: LivePreviewInstall;
let download: () => Promise<unknown>;

function project(root: string, engine = "lualatex", main = "main.tex") {
  return {
    root,
    files: [],
    manifest: { engine, rootDocuments: [{ path: main, name: "Main paper", isDefault: true }] },
  } as unknown as ProjectSnapshot;
}

function open(root: string, engine?: string, main?: string) {
  return renderHook(
    ({ snapshot }) => useLivePreview({ project: snapshot, activeFile: main ?? "main.tex", source: "", savedSource: "" }),
    { initialProps: { snapshot: project(root, engine, main) } },
  );
}

const commands = () => vi.mocked(invoke).mock.calls.map(([command]) => command);

beforeEach(() => {
  localStorage.clear();
  started.length = 0;
  sessions.length = 0;
  install = { ...READY };
  download = async () => ({ ...install, installed: true });
  // jsdom has no idle callbacks; run the deferred download on the next task.
  Object.assign(window, { requestIdleCallback: (run: () => void) => setTimeout(run, 0), cancelIdleCallback: clearTimeout });
  vi.mocked(invoke).mockReset().mockImplementation(async (command: string) => {
    if (command === "live_preview_status") return install;
    if (command === "live_preview_install") return download();
    if (command === "live_preview_remove") return { ...install, installed: false };
    return null;
  });
});
afterEach(() => {
  Reflect.deleteProperty(window, "requestIdleCallback");
  Reflect.deleteProperty(window, "cancelIdleCallback");
});

it("is on by default in a LuaLaTeX project: downloads rtex in the background and starts it", async () => {
  const { result } = open("/papers/lua");
  expect(result.current.enabled).toBe(true);
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  expect(commands()).toEqual(["live_preview_status", "live_preview_install"]);
  expect(started).toEqual(["/papers/lua"]);
});

it("is not offered to pdfLaTeX projects and asks the host nothing", async () => {
  const { result } = open("/papers/pdf", "pdf");
  expect(result.current.available).toBe(false);
  await act(async () => {});
  expect(result.current.state.kind).toBe("off");
  expect(commands()).toEqual([]);
});

it.each([
  ["an Intel-based Mac", { supported: false, lualatex: false }],
  ["a Mac without a working LuaLaTeX", { supported: true, lualatex: false }],
])("downloads nothing on %s and keeps the PDF panel quiet", async (_host, host) => {
  install = { ...READY, ...host };
  const { result } = open("/papers/lua");
  await waitFor(() => expect(result.current.state.kind).toBe("unavailable"));
  expect(commands()).toEqual(["live_preview_status"]);
  expect(started).toEqual([]);
});

it("starts on a main document in a subfolder: rtex typesets it from its own folder, as the build does", async () => {
  // A LuaLaTeX statement in cv/sop with \bibliography{../../references}.
  install = { ...READY, installed: true };
  const { result } = open("/papers/personal", "lualatex", "cv/sop/sop.tex");
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  expect(started).toEqual(["/papers/personal:cv/sop/sop.tex"]);
});

it("falls back quietly when the download fails, and retries on request", async () => {
  download = async () => { throw new Error("Could not download the rtex live preview engine: offline"); };
  const { result } = open("/papers/lua");
  await waitFor(() => expect(result.current.state).toEqual({
    kind: "download-failed",
    message: "Could not download the rtex live preview engine: offline",
  }));
  expect(started).toEqual([]);
  download = async () => ({ ...install, installed: true });
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  expect(commands().filter((command) => command === "live_preview_install")).toHaveLength(2);
});

it("keeps a project the writer turned off off, without asking the host", async () => {
  persistLivePreviewEnabled("/papers/lua", false);
  const { result } = open("/papers/lua");
  await act(async () => {});
  expect(result.current.enabled).toBe(false);
  expect(result.current.state.kind).toBe("off");
  expect(commands()).toEqual([]);
});

it("turning it off sticks across reopening, and removing rtex turns the default off", async () => {
  install = { ...READY, installed: true };
  const first = open("/papers/a");
  await waitFor(() => expect(first.result.current.state.kind).toBe("running"));
  act(() => first.result.current.setEnabled(false));
  expect(first.result.current.state.kind).toBe("off");
  first.unmount();
  expect(open("/papers/a").result.current.enabled).toBe(false);

  const second = open("/papers/b");
  expect(second.result.current.enabled).toBe(true);
  await act(async () => { await second.result.current.remove(); });
  expect(second.result.current.enabled).toBe(false);
  second.unmount();
  // Not on by default anywhere once rtex is removed, until chosen again.
  expect(open("/papers/c").result.current.enabled).toBe(false);
  const chosen = open("/papers/b");
  act(() => chosen.result.current.setEnabled(true));
  expect(chosen.result.current.enabled).toBe(true);
});


it("starts rtex again when it exits on its own, instead of staying stopped", async () => {
  const { result } = open("/papers/lua");
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  act(() => sessions[0].exit(9));
  await waitFor(() => expect(started).toHaveLength(2));
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  expect((sessions[0] as unknown as { dispose: ReturnType<typeof vi.fn> }).dispose).toHaveBeenCalled();
});

it("stays stopped with the reason when rtex keeps exiting, until the writer restarts it", async () => {
  const { result } = open("/papers/lua");
  for (let restart = 0; restart < MAX_RESTARTS; restart++) {
    await waitFor(() => expect(result.current.state.kind).toBe("running"));
    act(() => sessions[restart].exit(9));
    await waitFor(() => expect(started).toHaveLength(restart + 2));
  }
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  act(() => sessions[MAX_RESTARTS].exit(9));
  await waitFor(() => expect(result.current.state).toMatchObject({ kind: "failed" }));
  const message = (result.current.state as { message: string }).message;
  expect(message).toContain(`rtex stopped ${MAX_RESTARTS + 1} times`);
  expect(message).toContain("signal 9");
  expect(message).toContain("rtex: out of memory");
  expect(started).toHaveLength(MAX_RESTARTS + 1);
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  expect(started).toHaveLength(MAX_RESTARTS + 2);
  act(() => sessions[MAX_RESTARTS + 1].exit(9));
  await waitFor(() => expect(started).toHaveLength(MAX_RESTARTS + 3));
});

it("counts an exit once, and once stopped stays stopped: a later report neither restarts nor counts it", async () => {
  const now = vi.spyOn(Date, "now");
  let clock = 1_000_000;
  now.mockImplementation(() => clock);
  try {
    const { result } = open("/papers/lua");
    await waitFor(() => expect(result.current.state.kind).toBe("running"));
    act(() => {
      sessions[0].exit(9);
      sessions[0].report();
    });
    await waitFor(() => expect(started).toHaveLength(2));
    for (let restart = 1; restart < MAX_RESTARTS; restart++) {
      await waitFor(() => expect(result.current.state.kind).toBe("running"));
      act(() => sessions[restart].exit(9));
      await waitFor(() => expect(started).toHaveLength(restart + 2));
    }
    await waitFor(() => expect(result.current.state.kind).toBe("running"));
    const last = sessions[MAX_RESTARTS];
    act(() => last.exit(9));
    await waitFor(() => expect(result.current.state.kind).toBe("failed"));
    expect(last.dispose).toHaveBeenCalled();
    expect(result.current.buildHooks.current).toBeNull();
    clock += 6 * 60_000;
    act(() => last.report());
    await act(async () => {});
    expect(result.current.state.kind).toBe("failed");
    expect(started).toHaveLength(MAX_RESTARTS + 1);
  } finally {
    now.mockRestore();
  }
});

it("starts a project opened after another's crashes with no restart history", async () => {
  const { result, rerender } = open("/papers/a");
  for (let restart = 0; restart < MAX_RESTARTS; restart++) {
    await waitFor(() => expect(result.current.state.kind).toBe("running"));
    act(() => sessions[restart].exit(9));
    await waitFor(() => expect(started).toHaveLength(restart + 2));
  }
  rerender({ snapshot: project("/papers/b") });
  await waitFor(() => expect(started.at(-1)).toBe("/papers/b"));
  await waitFor(() => expect(result.current.state.kind).toBe("running"));
  act(() => sessions.at(-1)!.exit(9));
  await waitFor(() => expect(started.filter((root) => root === "/papers/b")).toHaveLength(2));
});
