/**
 * Finder drops onto the Agent panel (`drops.ts`, lattice#1254): any file
 * lights the panel and goes to the composer, whatever its format.
 *
 *   pnpm vitest run --config gpui/islands/vitest.config.ts
 */
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/app/notify", () => ({ showError: vi.fn() }));
vi.mock("../../../src/app-utils", () => ({
  toMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
}));
vi.mock("../shared/host", () => ({ host: vi.fn() }));

const { agentFileDragListener } = await import("./drops");

const DROPPED = [
  "/Users/someone/Downloads/report.docx",
  "/Users/someone/Downloads/budget.xlsx",
  "/Users/someone/Downloads/trace.qzx",
];

function harness(read: (paths: string[]) => Promise<{ name: string; mimeType: string; bytesBase64: string }[]>) {
  const lit: boolean[] = [];
  const posted: object[] = [];
  const failed: string[] = [];
  const reads: string[][] = [];
  const listen = agentFileDragListener(
    (active) => lit.push(active),
    (message) => posted.push(message),
    (paths) => {
      reads.push(paths);
      return read(paths);
    },
    (message) => failed.push(message),
  );
  return { listen, lit, posted, failed, reads };
}

describe("agentFileDragListener", () => {
  it("lights the panel for files of any format, and puts it out on leaving", async () => {
    const { listen, lit, reads } = harness(async () => []);
    await listen({ phase: "enter", paths: DROPPED });
    await listen({ phase: "over", paths: [] });
    await listen({ phase: "leave", paths: [] });
    await listen({ phase: "enter", paths: [] });
    expect(lit).toEqual([true, true, false, false]);
    expect(reads).toEqual([]);
  });

  it("reads a drop's paths alone and posts every file to the composer", async () => {
    const { listen, lit, posted, failed, reads } = harness(async (paths) =>
      paths.map((path) => ({
        name: path.split("/").pop()!,
        mimeType: "application/octet-stream",
        bytesBase64: btoa("PK\u0003\u0004"),
      })),
    );
    await listen({ phase: "enter", paths: ["/elsewhere/old.png"] });
    await listen({ phase: "drop", paths: DROPPED });
    expect(lit).toEqual([true, false]);
    expect(reads).toEqual([DROPPED]);
    expect(failed).toEqual([]);
    expect(posted).toHaveLength(1);
    const message = posted[0] as { type: string; files: { name: string; mimeType: string; bytes: ArrayBuffer }[] };
    expect(message.type).toBe("lattice:composer-files");
    expect(message.files.map(({ name, mimeType }) => [name, mimeType])).toEqual([
      ["report.docx", "application/octet-stream"],
      ["budget.xlsx", "application/octet-stream"],
      ["trace.qzx", "application/octet-stream"],
    ]);
    expect([...new Uint8Array(message.files[0].bytes)]).toEqual([0x50, 0x4b, 0x03, 0x04]);
  });

  it("shows the owner's refusal and posts nothing", async () => {
    const { listen, posted, failed } = harness(async () => {
      throw new Error("figures is a folder. Attach the files inside it instead.");
    });
    await listen({ phase: "drop", paths: ["/Users/someone/figures"] });
    expect(posted).toEqual([]);
    expect(failed).toEqual(["figures is a folder. Attach the files inside it instead."]);
  });

  it("does nothing for a drop that names no files", async () => {
    const { listen, posted, reads } = harness(async () => []);
    await listen({ phase: "drop", paths: [] });
    await listen({ phase: "drop" });
    expect(reads).toEqual([]);
    expect(posted).toEqual([]);
  });
});
