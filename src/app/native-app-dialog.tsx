import { useEffect, useReducer, useRef } from "react";
import { useLingui } from "@lingui/react/macro";
import { Channel, invoke } from "@tauri-apps/api/core";
import { HardDrive, MinusCircle, Zap } from "lucide-react";
import { toMessage } from "../app-utils";
import { Button } from "../components/ui/button";
import { InlineMessage } from "../components/ui/inline-message";
import { ModalDialog } from "../components/ui/modal-dialog";
import { restartLattice, runningWorkStoppedByRestart } from "../telemetry/app-restart";
import type { DownloadEvent, UpdateClaim } from "../telemetry/app-updater";
import { releaseNativeApp, type NativeAppRelease } from "./native-app";
import "./native-app-dialog.css";

/**
 * Where the move stands. `installed` is a move whose restart waits: work a
 * restart would stop is running (`running`), or a window could not save, so
 * the native app opens on Restart now or at the next launch.
 */
type Step =
  | { kind: "downloading"; progress: number | null }
  | { kind: "installing" }
  | { kind: "restarting" }
  | { kind: "installed"; running: string | null }
  | { kind: "failed"; reason: string | null };

/** Steps a close would cut short: the bundle is being replaced, or Lattice is restarting into it. */
const BUSY: ReadonlySet<Step["kind"]> = new Set(["downloading", "installing", "restarting"]);

/**
 * The notice while Lattice moves to its native version: what changes, what
 * comes along, what isn't there yet, and the download's progress. It starts
 * at once and offers no way to stay; a failure says why and tries again on
 * Try again or at the next launch.
 */
export function NativeAppDialog({ release, onClose }: { release: NativeAppRelease; onClose: () => void }) {
  const { t } = useLingui();
  const [step, setStep] = useReducer((_: Step, next: Step) => next, { kind: "downloading", progress: null });
  const busy = BUSY.has(step.kind);
  const started = useRef(false);

  // The backend holds the release until told to let go of it.
  useEffect(() => () => releaseNativeApp(release), [release]);

  const restart = (running: string | null) => void restartInto(running, setStep);
  const move = () => void moveToNative(release, setStep);

  useEffect(() => {
    // Once per notice, even when React runs the effect twice; Try again
    // starts it over.
    if (started.current) return;
    started.current = true;
    void moveToNative(release, setStep);
  }, [release]);

  const running = step.kind === "installed" ? step.running : null;
  const percent = step.kind === "downloading" && step.progress !== null ? Math.round(step.progress * 100) : null;
  const progressLabel = step.kind === "downloading"
    ? (percent === null ? t`Downloading the native Lattice…` : t`Downloading the native Lattice… ${percent}%`)
    : step.kind === "installing" ? t`Installing…` : t`Restarting…`;

  return (
    <ModalDialog label={t`Lattice is moving to its native version`} onClose={onClose} closeDisabled={busy}>
      <div className="modal whats-new-modal native-app-modal">
        <header className="whats-new-header">
          <h2>{t`Lattice is moving to its native version`}</h2>
        </header>
        <ol className="whats-new-list">
          <li>
            <span className="whats-new-icon" aria-hidden="true"><Zap size={15} aria-hidden /></span>
            <div>
              <strong>{t`What’s new`}</strong>
              <p>{t`From this version, Lattice on the Mac is the native app: drawn by your Mac instead of as a web page, faster and lighter, with the same look and the same ways of working.`}</p>
            </div>
          </li>
          <li>
            <span className="whats-new-icon" aria-hidden="true"><HardDrive size={15} aria-hidden /></span>
            <div>
              <strong>{t`Your data`}</strong>
              <p>{t`Your recent projects, settings, themes, plugins, templates, Agent skills, Agent conversations and Overleaf links come along. Your projects stay where they are.`}</p>
            </div>
          </li>
          <li>
            <span className="whats-new-icon" aria-hidden="true"><MinusCircle size={15} aria-hidden /></span>
            <div>
              <strong>{t`Not there yet`}</strong>
              <p>{t`No grammar check and no dictionaries for other languages: spell check uses macOS’s own checker.`}</p>
            </div>
          </li>
        </ol>
        {busy && (
          <div className="native-app-progress" aria-live="polite">
            <div className="native-app-progress-track" role="progressbar" aria-label={t`Move progress`}
              aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}>
              <div className={percent === null ? "native-app-progress-fill native-app-progress-indeterminate" : "native-app-progress-fill"}
                style={percent === null ? undefined : { width: `${percent}%` }} />
            </div>
            <span>{progressLabel}</span>
          </div>
        )}
        {step.kind === "failed" && (
          <InlineMessage level="error">
            {step.reason ?? t`Another Lattice window is installing an update.`} {t`Lattice tries again the next time it starts.`}
          </InlineMessage>
        )}
        {step.kind === "installed" && (
          <InlineMessage level="success">
            {running
              ? t`The native Lattice is installed. Restarting now stops ${running}; otherwise it opens the next time Lattice starts.`
              : t`The native Lattice is installed and opens the next time Lattice starts.`}
          </InlineMessage>
        )}
        {(step.kind === "failed" || step.kind === "installed") && (
          <div className="modal-actions native-app-actions">
            <span />
            <span className="native-app-actions-end">
              <Button variant="secondary" onClick={onClose}>{t`Close`}</Button>
              {step.kind === "failed" ? (
                <Button variant="primary" onClick={move} autoFocus>{t`Try again`}</Button>
              ) : (
                <Button variant="primary" onClick={() => restart(step.running)} autoFocus>{t`Restart now`}</Button>
              )}
            </span>
          </div>
        )}
      </div>
    </ModalDialog>
  );
}

/** Restart into the installed native app; a window that could not save leaves it for Restart now or the next launch. */
async function restartInto(running: string | null, setStep: (step: Step) => void) {
  setStep({ kind: "restarting" });
  try {
    await restartLattice();
  } catch {
    // A window could not save an edit and says so itself; the native app
    // is in place and opens at the next launch, or on Restart now.
    setStep({ kind: "installed", running });
  }
}

/**
 * Install the native app over this one, then restart into it unless that
 * would stop running work. The claim is the updater's own (app_restart.rs),
 * under "native <version>" so a classic release of the same number never
 * reads as this install.
 */
async function moveToNative(release: NativeAppRelease, setStep: (step: Step) => void) {
  const version = `native ${release.version}`;
  setStep({ kind: "downloading", progress: null });
  const claim = await invoke<UpdateClaim>("claim_update_install", { version }).catch(() => "claimed" as const);
  if (claim === "busy") {
    setStep({ kind: "failed", reason: null });
    return;
  }
  if (claim === "claimed") {
    let total = 0;
    let received = 0;
    const channel = new Channel<DownloadEvent>();
    channel.onmessage = (event) => {
      if (event.event === "Started") total = event.data?.contentLength ?? 0;
      else if (event.event === "Progress") {
        received += event.data.chunkLength;
        if (total > 0) setStep({ kind: "downloading", progress: Math.min(1, received / total) });
      } else setStep({ kind: "installing" });
    };
    try {
      await invoke("install_update", { version, rid: release.rid, onEvent: channel });
    } catch (reason) {
      setStep({ kind: "failed", reason: toMessage(reason) });
      return;
    }
  }
  const running = await runningWorkStoppedByRestart();
  if (running) setStep({ kind: "installed", running });
  else await restartInto(null, setStep);
}
