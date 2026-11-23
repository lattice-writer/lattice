/**
 * The live preview's state, as a small pill over the PDF: downloading rtex,
 * starting, the first layout, live, a layout catching up, or stopped; a first
 * layout that never comes says so a minute after the preamble loaded and, if it
 * persists, offers to reinstall rtex (a fresh download and build folders). Its dot
 * pulses while a quiet build brings the full PDF up to date. It
 * floats over the pages rather than joining the PDF toolbar, so it costs the
 * toolbar no width in a narrow panel. Live preview is on by default, so a Mac
 * that cannot run it or a download that failed shows nothing here: the PDF
 * just updates with each build, and Settings says why.
 */
import { useSyncExternalStore } from "react";
import { useLingui } from "@lingui/react/macro";
import { RotateCw } from "lucide-react";
import { Tip } from "../components/icon-tip";
import type { LivePreviewSession, LivePreviewSnapshot } from "./live-preview-session";
import type { LivePreviewState } from "./use-live-preview";
import "./live-preview.css";

type Tone = "busy" | "live" | "pending" | "stopped";

type Actions = { onRetry: () => void; onReinstall: () => void };

export function LivePreviewStatus({ state, ...actions }: { state: LivePreviewState } & Actions) {
  if (state.kind === "off" || state.kind === "unavailable" || state.kind === "download-failed") return null;
  if (state.kind === "running") return <RunningStatus session={state.session} {...actions} />;
  return <StatusPill state={state} snapshot={null} {...actions} />;
}

function RunningStatus({ session, ...actions }: { session: LivePreviewSession } & Actions) {
  const snapshot = useSyncExternalStore(session.subscribe, session.getSnapshot);
  return <StatusPill state={{ kind: "running", session }} snapshot={snapshot} {...actions} />;
}

function StatusPill({ state, snapshot, onRetry, onReinstall }: {
  state: LivePreviewState;
  snapshot: LivePreviewSnapshot | null;
} & Actions) {
  const { t } = useLingui();
  const about = t`Live preview redraws the paragraph you are typing within milliseconds, with rtex and your own LuaLaTeX. Page breaks, references, the exported PDF and jumps to the source still come from the normal build.`;
  let tone: Tone = "busy";
  let label: string;
  let detail: string | null = null;
  // A click restarts a stopped preview, and reinstalls one still without a
  // first layout after a minute.
  let action: (() => void) | null = null;
  let actionLabel = t`Restart live preview`;
  if (state.kind === "downloading") {
    const percent = Math.round(state.progress * 100);
    label = t`Setting up live preview… ${percent}%`;
    detail = t`Downloading rtex once, about 4 MB. Builds work as usual meanwhile. You can turn it off in Settings › Editor & builds.`;
  } else if (snapshot?.overdue) {
    tone = "pending";
    action = onReinstall;
    actionLabel = t`Reinstall live preview`;
    label = t`Live preview is taking long`;
    detail = [
      t`rtex loaded your preamble over a minute ago and has not laid the document out yet.`,
      t`Something in the document may keep LuaLaTeX busy, such as a loop or a package waiting for input: that needs a fix in the document, which a reinstall will not bring. The PDF still updates with each build.`,
      t`If it persists with a document that compiles, click to reinstall live preview: rtex downloads again and lays the document out from scratch. You can also turn it off in Settings › Editor & builds.`,
    ].join("\n\n");
  } else if (state.kind === "starting" || snapshot?.phase === "starting") {
    label = t`Starting live preview…`;
    detail = t`Loading your preamble into LuaLaTeX.`;
  } else if (state.kind === "failed") {
    tone = "stopped";
    label = t`Live preview stopped`;
    detail = state.message;
  } else if (snapshot?.phase === "first-layout") {
    label = t`First layout…`;
    detail = t`Typesetting the whole document once. Typing shows here as soon as it finishes.`;
  } else if (snapshot?.phase === "exited") {
    tone = "stopped";
    label = t`Live preview stopped`;
    detail = snapshot.detail ?? t`rtex stopped. The PDF still updates with each build.`;
  } else if (snapshot?.phase === "uncompiled") {
    tone = "pending";
    label = t`Waiting for the document to compile`;
    detail = t`A TeX error stops the document before its first page. Live preview starts once it compiles.`;
  } else if (snapshot?.phase === "paused") {
    tone = "pending";
    label = t`Live typing paused`;
    detail = t`rtex could not load the preamble for live typing. The preview still updates after each full layout and build.`;
  } else if (snapshot?.layoutPending) {
    tone = "pending";
    label = t`Layout updating`;
    detail = t`The paragraph you typed is shown. Page breaks and what follows it catch up in a moment.`;
  } else {
    tone = "live";
    label = t`Live`;
  }
  const stopped = tone === "stopped";
  if (stopped) action = onRetry;
  // The pulse stands in for the Build button's spinner and the PDF's
  // "Updating…", which a quiet build does not show (quiet-builds.ts).
  const rebuilding = !stopped && snapshot?.rebuilding === true;
  const rebuildNote = rebuilding ? t`The full PDF is rebuilding in the background. Page breaks, references and jumps to the source catch up when it finishes.` : null;
  const tip = [label, detail, rebuildNote, about].filter(Boolean).join("\n\n");
  return (
    <div className="live-preview-status-host" data-rebuilding={rebuilding || undefined}>
      <Tip label={<span className="live-preview-status-tip">{tip}</span>} side="top">
        {action ? (
          <button type="button" className="live-preview-status" data-tone={tone} onClick={action} aria-label={t`${label}. ${actionLabel}`}>
            <span className="live-preview-status-dot" aria-hidden="true" />
            <span className="live-preview-status-label">{label}</span>
            <RotateCw size={11} aria-hidden="true" />
          </button>
        ) : (
          <span className="live-preview-status" data-tone={tone} role="status" tabIndex={0} aria-label={label}>
            <span className="live-preview-status-dot" aria-hidden="true" />
            <span className="live-preview-status-label">{label}</span>
          </span>
        )}
      </Tip>
    </div>
  );
}
