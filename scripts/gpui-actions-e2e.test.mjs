// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { ACTIONS_FILE, actionsNeeds, changedLines, readContext, statementAt } from "./gpui-actions-e2e.mjs";
import { readModules } from "./gpui-e2e-modules.mjs";

const before = [
  "use super::overleaf_flow::{OpenOnOverleaf, SyncOverleaf};",
  "",
  "impl Shell {",
  "    pub(crate) fn close_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {",
  "        self.workspace.close();",
  "    }",
  "",
  "    pub(crate) fn command_context(&self, cx: &App) -> CommandContext {",
  "        CommandContext {",
  "            overleaf_linked: self.overleaf.read(cx).link().is_some(),",
  "        }",
  "    }",
  "",
  "    pub(super) fn register_commands(&self, element: Stateful<gpui::Div>, cx: &mut Context<Self>) -> Stateful<gpui::Div> {",
  "        let element = element",
  "            .on_action(cx.listener(|this, _: &CloseTab, window, cx| this.close_tab(window, cx)))",
  "            .on_action(",
  "                cx.listener(|this, _: &SyncOverleaf, window, cx| this.sync_overleaf(window, cx)),",
  "            )",
  "            .on_action(cx.listener(|this, _: &Unused, window, cx| {",
  "                this.unused(window, cx);",
  "            }))",
  "            .on_action(cx.listener(|_, _: &ThemeDark, _, cx| {",
  "                crate::settings::update(cx, |settings| settings.dark = true)",
  "            }));",
  "        element",
  "    }",
  "}",
].join("\n");

const covers = new Map([
  ["overleaf", ["gpui/crates/lattice-gpui/src/shell/overleaf_flow.rs"]],
  ["palette", ["gpui/crates/lattice-gpui/src/shell/commands"]],
  ["shell_lifecycle", ["gpui/crates/lattice-gpui/src/shell/menus.rs"]],
  ["settings", ["gpui/crates/lattice-gpui/src/settings"]],
  ["tabs", []],
  ["keys", []],
]);
const context = {
  scenario: [
    { file: "gpui/crates/lattice-gpui/src/scenario/e2e/tabs.rs", text: "window.dispatch_action(Box::new(CloseTab), cx)" },
    { file: "gpui/crates/lattice-gpui/src/scenario/e2e/keys/mod.rs", text: 'self.press("cmd-w")' },
    { file: "gpui/crates/lattice-gpui/src/scenario/shell_steps.rs", text: "ThemeDark" },
  ],
  keys: new Map([["CloseTab", ["cmd-w"]]]),
  methods: new Map([["sync_overleaf", ["gpui/crates/lattice-gpui/src/shell/overleaf_flow.rs"]]]),
};

/** A one-hunk -U0 diff replacing old lines [from, from + drop) with `add`. */
function edit(from, drop, add) {
  const lines = before.split("\n");
  const after = [...lines.slice(0, from - 1), ...add, ...lines.slice(from - 1 + drop)].join("\n");
  const diff = `@@ -${drop ? from : from - 1},${drop} +${add.length ? from : from - 1},${add.length} @@\n`;
  return actionsNeeds({ diff, before, after }, covers, context);
}

describe("gpui-actions-e2e", () => {
  it("reads a -U0 diff's changed lines on both sides", () => {
    expect(changedLines("@@ -3,2 +3 @@\n@@ -9,0 +10,2 @@\n")).toEqual({ added: [3, 10, 11], removed: [3, 4] });
  });

  it("places a line in its statement", () => {
    const lines = before.split("\n");
    expect(statementAt(lines, 1).kind).toBe("use");
    expect(statementAt(lines, 5)).toEqual({ kind: "handler", fn: "close_tab" });
    expect(statementAt(lines, 10).kind).toBe("context");
    expect(statementAt(lines, 15).kind).toBe("other");
    expect(statementAt(lines, 18)).toMatchObject({ kind: "binding" });
    expect(statementAt(lines, 18).text).toContain("SyncOverleaf");
    expect(statementAt(lines, 21).text).toContain("&Unused");
    expect(statementAt(lines, 22).text).toContain("&Unused");
  });

  it("runs the smoke flow alone for an import or a comment", () => {
    expect(edit(1, 1, ["use super::overleaf_flow::{OpenOnOverleaf, PauseSync, SyncOverleaf};"])).toEqual({ full: false, modules: [], why: [] });
    expect(edit(16, 0, ["            // Tabs."])).toMatchObject({ full: false, modules: [] });
  });

  it("maps a binding to the modules that name its action, press its key or cover the method it calls", () => {
    expect(edit(16, 1, ["            .on_action(cx.listener(|this, _: &CloseTab, window, cx| this.close_tab(window, cx)).boxed())"])).toMatchObject({ full: false, modules: ["keys", "tabs"] });
    expect(edit(18, 1, ["                cx.listener(|this, _: &SyncOverleaf, window, cx| this.sync_overleaf(cx)),"])).toMatchObject({ full: false, modules: ["overleaf"] });
    expect(edit(24, 1, ["                crate::settings::update(cx, |settings| settings.dark = false)"])).toMatchObject({ full: false, modules: ["settings"] });
  });

  it("maps command_context to the palette and the menus, and a handler to its bindings' modules", () => {
    expect(edit(10, 1, ["            overleaf_linked: self.overleaf.read(cx).linked(),"])).toMatchObject({ full: false, modules: ["palette", "shell_lifecycle"] });
    expect(edit(5, 1, ["        self.workspace.close_focused();"])).toMatchObject({ full: false, modules: ["keys", "tabs"] });
  });

  it("runs the smoke flow for a new binding no step reaches, and the whole scenario for an edit of one", () => {
    expect(edit(20, 0, ["            .on_action(cx.listener(|this, _: &PauseSync, _, cx| this.pause(cx)))"])).toMatchObject({ full: false, modules: [] });
    expect(edit(21, 1, ["                this.unused_now(window, cx);"])).toMatchObject({ full: true });
    expect(edit(15, 1, ["        let element = element.id(\"shell\")"])).toMatchObject({ full: true });
  });

  it("reads the real tree: keymap keys, shell methods and every binding's statement", () => {
    const real = readContext();
    expect(real.keys.get("CloseTab")).toContain("cmd-w");
    expect(real.methods.get("sync_overleaf")).toEqual(["gpui/crates/lattice-gpui/src/shell/overleaf_flow.rs"]);
    const after = readFileSync(ACTIONS_FILE, "utf8").split("\n");
    const bindings = after.flatMap((line, at) => (/^\s*\.on_action\(/.test(line) ? [statementAt(after, at + 1)] : []));
    expect(bindings.length).toBeGreaterThan(60);
    expect(bindings.every((statement) => statement.kind === "binding" && /&(?:[a-z_]+::)*[A-Z]\w*|Self::\w+/.test(statement.text))).toBe(true);
    const sync = after.findIndex((line) => line.includes("&SyncOverleaf"));
    expect(actionsNeeds({ diff: `@@ -${sync + 1} +${sync + 1} @@\n`, before: after.join("\n"), after: after.join("\n") }, readModules(), real).modules).toContain("overleaf");
  });
});
