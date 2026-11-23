import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { LivePreviewSession, LivePreviewSnapshot } from "./live-preview-session";
import { LivePreviewStatus } from "./live-preview-status";

afterEach(cleanup);

function running(patch: Partial<LivePreviewSnapshot>) {
  const snapshot: LivePreviewSnapshot = {
    phase: "first-layout", layoutPending: false, detail: null, exit: null, rebuilding: false, overdue: false, ...patch,
  };
  const session = { subscribe: () => () => {}, getSnapshot: () => snapshot } as unknown as LivePreviewSession;
  return { kind: "running" as const, session };
}

describe("the live preview pill", () => {
  it("spins on the first layout, then after a minute without one says so and reinstalls rtex on a click", () => {
    const onRetry = vi.fn();
    const onReinstall = vi.fn();
    const { rerender } = render(<LivePreviewStatus state={running({})} onRetry={onRetry} onReinstall={onReinstall} />);
    expect(screen.getByRole("status").textContent).toBe("First layout…");
    expect(screen.queryByRole("button")).toBeNull();
    rerender(<LivePreviewStatus state={running({ overdue: true })} onRetry={onRetry} onReinstall={onReinstall} />);
    const pill = screen.getByRole("button", { name: "Live preview is taking long. Reinstall live preview" });
    expect(pill.getAttribute("data-tone")).toBe("pending");
    fireEvent.click(pill);
    expect(onReinstall).toHaveBeenCalledTimes(1);
    expect(onRetry).not.toHaveBeenCalled();
  });

  it("restarts a stopped preview on a click", () => {
    const onRetry = vi.fn();
    render(<LivePreviewStatus state={running({ phase: "exited" })} onRetry={onRetry} onReinstall={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Live preview stopped. Restart live preview" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});
