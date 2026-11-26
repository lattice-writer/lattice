import { describe, expect, it } from "vitest";
import { agentFolderView, fromAgentFolder, toAgentFolder } from "./agent-folder-view";

const view = agentFolderView("/Users/ada/ProjectA/Manuscript", "/Users/ada/ProjectA");

describe("agentFolderView", () => {
  it("is the project's place inside the Agent's folder, and none without a folder above it", () => {
    expect(view).toEqual({ projectRoot: "/Users/ada/ProjectA/Manuscript", agentRoot: "/Users/ada/ProjectA", prefix: "Manuscript/" });
    expect(agentFolderView("/Users/ada/ProjectA/Manuscript", "/Users/ada/ProjectA/Manuscript")).toBeNull();
    expect(agentFolderView("/Users/ada/ProjectA/Manuscript", "/Users/ada/Project")).toBeNull();
    expect(agentFolderView("/Users/ada/ProjectA", "/Users/ada/ProjectA/Manuscript")).toBeNull();
    expect(agentFolderView("/Users/ada/ProjectA/Manuscript", null)).toBeNull();
    expect(agentFolderView("C:\\Docs\\ProjectA\\papers\\Manuscript", "C:\\Docs\\ProjectA")?.prefix).toBe("papers/Manuscript/");
  });
});

describe("the frame's messages", () => {
  it("reach the frame with the Agent's folder and its paths", () => {
    const context = {
      type: "lattice:host-context",
      workspaceRoot: "/Users/ada/ProjectA/Manuscript",
      editor: { path: "main.tex", line: 3, column: 0 },
      pdf: { page: 1, pageCount: 4, path: "/Users/ada/ProjectA/Manuscript/main.pdf" },
      writes: [{ path: "./refs.bib", writtenAt: "now" }],
      text: "main.tex",
    };
    expect(toAgentFolder(view, context)).toEqual({
      type: "lattice:host-context",
      workspaceRoot: "/Users/ada/ProjectA",
      editor: { path: "Manuscript/main.tex", line: 3, column: 0 },
      pdf: { page: 1, pageCount: 4, path: "/Users/ada/ProjectA/Manuscript/main.pdf" },
      writes: [{ path: "Manuscript/refs.bib", writtenAt: "now" }],
      text: "main.tex",
    });
    expect(toAgentFolder(null, context)).toBe(context);
  });

  it("come back as the project's, and a path outside the project reaches no project file", () => {
    const request = {
      type: "synara:turn-activity",
      workspaceRoot: "/Users/ada/ProjectA/",
      tool: { paths: ["Manuscript/figures/plot.tex", "analysis/results.R", "https://example.org/a"] },
      args: { path: "Manuscript/data.lattice-sheet" },
      filePath: "Manuscript\\main.tex",
    };
    expect(fromAgentFolder(view, request)).toEqual({
      type: "synara:turn-activity",
      workspaceRoot: "/Users/ada/ProjectA/Manuscript",
      tool: { paths: ["figures/plot.tex", "../analysis/results.R", "https://example.org/a"] },
      args: { path: "data.lattice-sheet" },
      filePath: "main.tex",
    });
    expect(fromAgentFolder(view, { workspaceRoot: "/elsewhere", filePath: "/Users/ada/ProjectA/analysis/results.R" }))
      .toEqual({ workspaceRoot: "/elsewhere", filePath: "/Users/ada/ProjectA/analysis/results.R" });
    expect(fromAgentFolder(view, "plain")).toBe("plain");
  });

  it("leave a page under `document` as it is, and translate the figure tool's document path", () => {
    const expand = { type: "synara:expand-visual", title: "Plot", html: "<p>x</p>", document: "<!doctype html>\n<html></html>" };
    expect(fromAgentFolder(view, expand)).toEqual(expand);
    expect(fromAgentFolder(view, { args: { document: "Manuscript/main.tex" } })).toEqual({ args: { document: "main.tex" } });
  });
});
