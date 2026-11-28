import { afterEach, describe, expect, it, vi } from "vitest";
import { startBrowserHostBridge } from "./browser-host-bridge";
import { FakeWebSocket, lastSocket, sockets } from "./fake-websocket";

const IPC_SERIALIZE_KEY = "__TAURI_TO_IPC_KEY__";

const NativeWebSocket = globalThis.WebSocket;
const nativeInternals = (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;

afterEach(() => {
  sockets.length = 0;
  vi.stubGlobal("WebSocket", NativeWebSocket);
  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = nativeInternals;
});

describe("browser host bridge channels", () => {
  it("forwards Tauri channel ordering envelopes without consuming them in the hidden host", async () => {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    const callbacks = new Map<number, (payload: unknown) => void>();
    let nextCallbackId = 100;
    const unregisterCallback = vi.fn((id: number) => callbacks.delete(id));
    const payloads = [
      { index: 0, message: { event: "Started", data: { contentLength: 400 } } },
      { index: 1, message: { event: "Progress", data: { chunkLength: 100 } } },
      { index: 2, end: true },
    ];
    const invoke = vi.fn(async (command: string, args?: unknown) => {
      if (command !== "plugin:updater|download_and_install") return undefined;
      const channel = (args as { onEvent: Record<string, () => string> }).onEvent;
      const serialized = channel[IPC_SERIALIZE_KEY]();
      const hostCallbackId = Number(serialized.replace("__CHANNEL__:", ""));
      for (const payload of payloads) callbacks.get(hostCallbackId)?.(payload);
      return undefined;
    });
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
      invoke,
      transformCallback(callback?: (payload: unknown) => void) {
        const id = nextCallbackId++;
        callbacks.set(id, callback ?? (() => {}));
        return id;
      },
      unregisterCallback,
    };

    startBrowserHostBridge({ token: "secret", port: 18_452 });
    const socket = lastSocket();
    socket.message({ type: "ready" });
    socket.message({
      type: "invoke",
      id: 7,
      command: "plugin:updater|download_and_install",
      args: { onEvent: "__CHANNEL__:42" },
      options: null,
    });

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledOnce());
    await vi.waitFor(() => {
      const messages = socket.send.mock.calls
        .map(([value]) => JSON.parse(String(value)) as { type: string })
        .filter((message) => message.type === "callback");
      // Every ordering envelope reaches the tab, in order, under the tab's channel id.
      expect(messages).toEqual(payloads.map((payload) => ({ type: "callback", id: 42, payload })));
    });
    expect(unregisterCallback).toHaveBeenCalledWith(100);
  });
});

describe("a headless host's bridge", () => {
  /** Start the hidden host's bridge and return what it answered to each invoke the tab sent. */
  async function answers(headless: boolean, commands: string[]) {
    vi.stubGlobal("WebSocket", FakeWebSocket);
    const invoke = vi.fn(async () => "done");
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
      invoke, transformCallback: () => 1, unregisterCallback: () => undefined,
    };
    startBrowserHostBridge({ token: "secret", port: 18_452, headless });
    const socket = lastSocket();
    socket.message({ type: "ready" });
    commands.forEach((command, id) => socket.message({ type: "invoke", id, command, args: {}, options: null }));
    const responses = () => socket.send.mock.calls
      .map(([value]) => JSON.parse(String(value)) as { type: string; id: number; ok: boolean; error?: unknown })
      .filter((message) => message.type === "response");
    await vi.waitFor(() => expect(responses()).toHaveLength(commands.length));
    return { invoke, responses: responses().sort((a, b) => a.id - b.id) };
  }

  const refused = ["plugin:opener|open_url", "plugin:opener|reveal_item_in_dir", "plugin:dialog|message", "plugin:dialog|open"];

  it("refuses the opener and the dialog plugin, even from an invoke that skips the tab's own answers", async () => {
    const { invoke, responses } = await answers(true, [...refused, "open_project_window"]);
    expect(responses.map(({ ok }) => ok)).toEqual([false, false, false, false, true]);
    expect(responses[0]?.error).toBe("This Lattice runs without a desktop, so it cannot open apps or folders on its computer.");
    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("open_project_window", {}, null);
  });

  it("passes them on for a host with a desktop", async () => {
    const { invoke, responses } = await answers(false, refused);
    expect(responses.every(({ ok }) => ok)).toBe(true);
    expect(invoke).toHaveBeenCalledTimes(refused.length);
  });
});
