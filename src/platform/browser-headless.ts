/**
 * What a tab of a headless browser host does itself. `--browser-host
 * --headless` runs Lattice on a Linux machine with no desktop session, so the
 * host never calls the system opener (browser_host.rs): nobody would see the
 * browser, file manager or window it started. This tab opens links in its own
 * browser instead, answers message and confirm dialogs itself, and refuses to
 * show the host's files and folders, a file picker or Overleaf's sign-in
 * window, saying why. Projects open in this tab
 * (`open_project_window`'s `openHere`). Only a host started
 * that way says `headless` in its session reply, and a tab of any other host
 * never loads this module.
 */
import { runtimeText } from "./browser-runtime";

// Bootstrap text like browser-runtime.ts's, kept here so only a headless
// host's tab loads it. The first is the host's own NO_DESKTOP_APPS
// (browser_host.rs).
/* eslint-disable lingui/no-unlocalized-strings -- self-translated bootstrap text, Tauri command names, the dialog plugin's button names */
const MESSAGES = {
  "no-desktop": [
    "This Lattice runs without a desktop, so it cannot open apps or folders on its computer.",
    "此 Lattice 在没有桌面的电脑上运行，无法在那台电脑上打开应用或文件夹。",
  ],
  "link-refused": [
    "Lattice opens only web, email and phone links.",
    "Lattice 只打开网页、电子邮件和电话链接。",
  ],
  // The host's own NO_DESKTOP_FILE_PICKER (browser_host/dialogs.rs) and NO_DESKTOP_SIGN_IN (browser_host.rs).
  "no-file-picker": [
    "This Lattice runs without a desktop, so it cannot show a file picker on its computer.",
    "此 Lattice 在没有桌面的电脑上运行，无法在那台电脑上显示文件选择窗口。",
  ],
  "no-sign-in-window": [
    "This Lattice runs without a desktop, so it cannot open the Overleaf sign-in window.",
    "此 Lattice 在没有桌面的电脑上运行，无法打开 Overleaf 登录窗口。",
  ],
  // The browser names its own buttons, so a dialog's own OK says what it does.
  "ok-means": ["OK: {label}", "确定：{label}"],
} as const satisfies Record<string, readonly [english: string, chinese: string]>;

const message = (key: keyof typeof MESSAGES) => runtimeText(MESSAGES[key]);

/** The opener plugin's URL scope (capabilities/default.json). */
const LINK_SCHEMES = new Set(["http:", "https:", "mailto:", "tel:"]);

const noDesktop = () => {
  throw new Error(message("no-desktop"));
};

/** Commands this tab answers itself rather than relay to the host. */
export const HEADLESS_COMMANDS = new Map<string, (args: Record<string, unknown>) => unknown>([
  // Runs inside the click that asked, so the browser lets it open a tab.
  ["plugin:opener|open_url", ({ url }) => {
    const link = typeof url === "string" ? parseUrl(url) : null;
    if (!link || !LINK_SCHEMES.has(link.protocol)) throw new Error(message("link-refused"));
    // The linked page gets no handle on this one.
    window.open(link.href, "_blank", "noopener,noreferrer");
  }],
  ["plugin:opener|open_path", noDesktop],
  ["plugin:opener|reveal_item_in_dir", noDesktop],
  // The native dialog would open on the host's hidden display and wait there.
  ["plugin:dialog|message", answerMessage],
  // So would a file picker. Most callers log a failure without showing it, and
  // treat null as Cancel, so the tab says why and answers as Cancel.
  ["plugin:dialog|open", noFilePicker],
  ["plugin:dialog|save", noFilePicker],
  ["overleaf_begin_login", () => {
    throw new Error(message("no-sign-in-window"));
  }],
]);

function noFilePicker(): null {
  window.alert(message("no-file-picker"));
  return null;
}

/** The OK and Cancel answers of `buttons`, as the dialog plugin names them, or null for a dialog with only OK. */
function dialogAnswers(buttons: unknown): [ok: string, cancel: string] | null {
  if (buttons === "OkCancel") return ["Ok", "Cancel"];
  if (buttons === "YesNo") return ["Yes", "No"];
  // A browser confirm has two answers. Mapping a third ("Don't save") onto
  // one of them would lose it without a word, so such a dialog fails here
  // until something needs one and this tab learns to ask it.
  if (buttons === "YesNoCancel") throw new Error(THREE_ANSWERS);
  if (typeof buttons !== "object" || buttons === null) return null;
  const { OkCancelCustom: okCancel, YesNoCancelCustom: yesNoCancel } = buttons as Record<string, unknown>;
  if (yesNoCancel !== undefined) throw new Error(THREE_ANSWERS);
  const labels = Array.isArray(okCancel) ? okCancel : null;
  return labels?.every((label) => typeof label === "string") ? [labels[0], labels[1]] : null;
}

/** For whoever adds the first three-answer dialog: not shown to people today. */
const THREE_ANSWERS = "A dialog with three answers cannot be shown in a headless host's tab yet.";

/**
 * `plugin:dialog|message`, which `message`, `ask` and `confirm` all send,
 * answered in this tab with the name of the button pressed, as the plugin
 * would: `confirm` and `ask` compare it with their OK label.
 */
function answerMessage({ title, message: text, buttons }: Record<string, unknown>): string {
  const body = [title, text].filter((part) => typeof part === "string" && part !== "").join("\n\n");
  const answers = dialogAnswers(buttons);
  if (!answers) {
    window.alert(body);
    const custom = (buttons as { OkCustom?: unknown } | null)?.OkCustom;
    return typeof custom === "string" ? custom : "Ok";
  }
  const [ok, cancel] = answers;
  const custom = ok !== "Ok" && ok !== "Yes";
  return window.confirm(custom ? `${body}\n\n${runtimeText(MESSAGES["ok-means"], { label: ok })}` : body) ? ok : cancel;
}

/* eslint-enable lingui/no-unlocalized-strings */

function parseUrl(url: string): URL | null {
  try {
    return new URL(url);
  } catch {
    return null;
  }
}
