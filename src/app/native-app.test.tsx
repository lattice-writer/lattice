import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * The backend is the seam: every command the move reaches goes through
 * `invoke`, answered here per command name, and the install's channel is
 * the one the dialog hands over.
 */
const backend = vi.hoisted(() => ({
  answers: new Map<string, (args: Record<string, unknown>) => unknown>(),
  calls: [] as Array<{ command: string; args: Record<string, unknown> }>,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string, args: Record<string, unknown> = {}) => {
    backend.calls.push({ command, args });
    const answer = backend.answers.get(command);
    if (!answer) throw new Error(`Command ${command} not found`);
    return answer(args);
  }),
  Channel: class<T> {
    onmessage: (message: T) => void = () => {};
  },
}));

import { NativeAppDialog } from "./native-app-dialog";
import { NativeAppMigration } from "./native-app-migration";

const RELEASE = { version: "0.20.0", rid: 7 };

const called = (command: string) => backend.calls.filter((call) => call.command === command);

function installs() {
  backend.answers.set("claim_update_install", () => "claimed");
  backend.answers.set("install_update", (args) => {
    const channel = args.onEvent as { onmessage: (event: unknown) => void };
    channel.onmessage({ event: "Started", data: { contentLength: 100 } });
    channel.onmessage({ event: "Progress", data: { chunkLength: 100 } });
    channel.onmessage({ event: "Finished" });
    return null;
  });
  backend.answers.set("restart_lattice", () => null);
}

beforeEach(() => {
  backend.answers.clear();
  backend.calls.length = 0;
  backend.answers.set("plugin:resources|close", () => null);
  backend.answers.set("restart_impact", () => ({ terminalCommands: 0, builds: 0, agentTurns: 0 }));
});
afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("when Lattice moves to the native app", () => {
  it("checks only when the backend says this launch moves", async () => {
    backend.answers.set("take_native_move", () => false);
    backend.answers.set("check_native_app", () => RELEASE);
    render(<NativeAppMigration whatsNewOpen={false} />);
    await waitFor(() => expect(called("take_native_move")).toHaveLength(1));
    await act(async () => {});
    expect(called("check_native_app")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says nothing when no release is found, so the next launch tries again", async () => {
    backend.answers.set("take_native_move", () => true);
    backend.answers.set("check_native_app", () => null);
    render(<NativeAppMigration whatsNewOpen={false} />);
    await waitFor(() => expect(called("check_native_app")).toHaveLength(1));
    await act(async () => {});
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("waits for What's new to close, then moves without asking", async () => {
    backend.answers.set("take_native_move", () => true);
    backend.answers.set("check_native_app", () => RELEASE);
    installs();
    const { rerender } = render(<NativeAppMigration whatsNewOpen />);
    await waitFor(() => expect(called("check_native_app")).toHaveLength(1));
    await act(async () => {});
    expect(screen.queryByRole("dialog")).toBeNull();
    rerender(<NativeAppMigration whatsNewOpen={false} />);
    expect(await screen.findByRole("dialog", { name: "Lattice is moving to its native version" })).toBeTruthy();
    await waitFor(() => expect(called("restart_lattice")).toHaveLength(1));
    // No way to stay on this version.
    expect(screen.queryByRole("button", { name: /Not now|Don’t ask again/ })).toBeNull();
  });
});

describe("the move's notice", () => {
  it("installs the native release under its own claim, then restarts into it", async () => {
    installs();
    render(<NativeAppDialog release={RELEASE} onClose={() => {}} />);
    await waitFor(() => expect(called("restart_lattice")).toHaveLength(1));
    // Never the same claim as a classic update to 0.20.0.
    expect(called("claim_update_install")[0]?.args).toEqual({ version: "native 0.20.0" });
    expect(called("install_update")[0]?.args).toMatchObject({ version: "native 0.20.0", rid: 7 });
  });

  it("installs but waits to restart while work is running", async () => {
    installs();
    backend.answers.set("restart_impact", () => ({ terminalCommands: 0, builds: 2, agentTurns: 0 }));
    render(<NativeAppDialog release={RELEASE} onClose={() => {}} />);
    expect(await screen.findByText(/Restarting now stops 2 compiles/)).toBeTruthy();
    expect(called("install_update")).toHaveLength(1);
    expect(called("restart_lattice")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Restart now" }));
    await waitFor(() => expect(called("restart_lattice")).toHaveLength(1));
  });

  it("restarts without downloading again when another window already installed it", async () => {
    backend.answers.set("claim_update_install", () => "installed");
    backend.answers.set("restart_lattice", () => null);
    render(<NativeAppDialog release={RELEASE} onClose={() => {}} />);
    await waitFor(() => expect(called("restart_lattice")).toHaveLength(1));
    expect(called("install_update")).toHaveLength(0);
  });

  it("says why a move failed and tries again", async () => {
    backend.answers.set("claim_update_install", () => "claimed");
    backend.answers.set("install_update", () => { throw new Error("signature mismatch"); });
    const onClose = vi.fn();
    render(<NativeAppDialog release={RELEASE} onClose={onClose} />);
    expect(await screen.findByText(/signature mismatch/)).toBeTruthy();
    expect(screen.getByText(/tries again the next time it starts/)).toBeTruthy();
    installs();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(called("restart_lattice")).toHaveLength(1));
    // The release the closed notice held is let go.
    cleanup();
    expect(called("plugin:resources|close").map((call) => call.args)).toContainEqual({ rid: 7 });
  });

  it("says another window is installing instead of installing alongside", async () => {
    backend.answers.set("claim_update_install", () => "busy");
    render(<NativeAppDialog release={RELEASE} onClose={() => {}} />);
    expect(await screen.findByText(/Another Lattice window is installing an update/)).toBeTruthy();
    expect(called("install_update")).toHaveLength(0);
  });
});
