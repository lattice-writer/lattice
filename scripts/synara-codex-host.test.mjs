import { expect, it } from "vitest";
import { verifyCodexHostProcess } from "./synara-codex-host.mjs";

// A minified fork bundle keeps the literals the fixes add, whatever it names the code around them.
const bundle = `const a=b("codex");function c(d){const e=f.id==="lattice"&&process.platform==="darwin"&&process.env.LATTICE_BIBLIOGRAPHY_SANDBOX!=="1";}g.on("data",h=>{a.warn("codex app-server stderr",{});});g.on("exit",(i,j)=>{a.info("codex app-server exit",{});});`;

it("accepts a bundle that carries every Codex host fix", () => {
  expect(() => verifyCodexHostProcess(bundle)).not.toThrow();
});

it.each([
  ["LATTICE_BIBLIOGRAPHY_SANDBOX"],
  ["codex app-server stderr"],
  ["codex app-server exit"],
])("fails closed when the bundle lost %s", (marker) => {
  expect(() => verifyCodexHostProcess(bundle.replace(marker, "x"))).toThrow(marker);
});
