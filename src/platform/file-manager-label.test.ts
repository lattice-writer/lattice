import { afterEach, expect, it, vi } from "vitest";
import { showInFileManager } from "./file-manager-label";

afterEach(() => { vi.restoreAllMocks(); });

it("names each host's own file manager", () => {
  const userAgent = vi.spyOn(navigator, "userAgent", "get");
  const label = (agent: string) => {
    userAgent.mockReturnValue(agent);
    return showInFileManager().message;
  };
  expect(label("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("Show in Finder");
  expect(label("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/157.0")).toBe("Show in File Explorer");
  expect(label("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("Show in File Manager");
});
