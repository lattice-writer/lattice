import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { fitEditorText } from "../../../settings/code-themes";
import { contrastRatio } from "../../../settings/theme-customization";
import { engineSchema } from "./engine-schema";
import { colorMarkView, shownColor } from "./inline-color-view";

const LIGHT_GROUNDS = ["#fafafa", "#fcf9f4", "#f6fbf6", "#f4faff", "#faf8ff"];
const DARK_GROUNDS = ["#1f1f22", "#1d1b18", "#191c1a", "#181c1f", "#1c1a1f"];

/** What WebKit's canvas paints for each colour these tests use. */
const PIXELS: Record<string, number[]> = {
  blue: [0, 0, 255, 255], yellow: [255, 255, 0, 255], navy: [0, 0, 128, 255],
  "oklch(30% 0.1 260)": [24, 41, 84, 255], "rgb(0 0 255 / 0.3)": [0, 0, 255, 77], transparent: [0, 0, 0, 0],
};

const normalized = (color: string) => {
  const probe = document.createElement("span").style;
  probe.color = color;
  return probe.color;
};

beforeAll(() => {
  let fill = "";
  const context = {
    set fillStyle(value: string) { fill = value; },
    get fillStyle() { return fill; },
    clearRect: () => {},
    fillRect: () => {},
    getImageData: () => ({ data: new Uint8ClampedArray(PIXELS[fill] ?? [0, 0, 0, 255]) }),
  };
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
});

afterAll(() => {
  vi.restoreAllMocks();
  delete document.documentElement.dataset.theme;
});

describe("text colours on the editor ground", () => {
  it("keeps a colour that already reads at 4.5:1 on every tint", () => {
    expect(fitEditorText("#0000ff", 1, "light")).toBeNull();
    expect(shownColor("blue", "light")).toBe("blue");
  });

  it.each([
    ["yellow", "light", LIGHT_GROUNDS], ["navy", "dark", DARK_GROUNDS], ["oklch(30% 0.1 260)", "dark", DARK_GROUNDS],
    ["rgb(0 0 255 / 0.3)", "light", LIGHT_GROUNDS],
  ] as const)("refits %s on a %s ground to 4.5:1 on every tint", (color, kind, grounds) => {
    const shown = shownColor(color, kind);
    expect(shown).toMatch(/^#[0-9a-f]{6}$/);
    for (const ground of grounds) expect(contrastRatio(shown, ground)).toBeGreaterThanOrEqual(4.5);
  });

  it("leaves colours that take their context, or hide on purpose, as written", () => {
    expect(shownColor("currentColor", "dark")).toBe("currentColor");
    expect(shownColor("color-mix(in srgb, currentColor 50%, blue)", "dark")).toBe("color-mix(in srgb, currentColor 50%, blue)");
    expect(shownColor("transparent", "dark")).toBe("transparent");
  });

  it("repaints a coloured run when the theme changes, keeping the authored colour on the span", async () => {
    document.documentElement.dataset.theme = "light";
    const mark = engineSchema().marks.latticeColor!.create({ color: "navy" });
    const view = colorMarkView()({ mark, HTMLAttributes: {} } as unknown as Parameters<ReturnType<typeof colorMarkView>>[0]);
    const dom = view.dom as HTMLElement;
    expect(dom.dataset.latticeColor).toBe("navy");
    expect(dom.style.color).toBe("navy");
    expect(view.ignoreMutation?.({ type: "attributes", target: dom } as unknown as MutationRecord)).toBe(true);

    document.documentElement.dataset.theme = "dark";
    await new Promise((resolve) => setTimeout(resolve));
    expect(dom.style.color).toBe(normalized(shownColor("navy", "dark")));
    expect(dom.dataset.latticeColor).toBe("navy");

    view.destroy?.();
    document.documentElement.dataset.theme = "light";
    await new Promise((resolve) => setTimeout(resolve));
    expect(dom.style.color).toBe(normalized(shownColor("navy", "dark")));
  });
});
