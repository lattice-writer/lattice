import { lazy, Suspense, useEffect, useReducer } from "react";
import { nativeMigrationRelease, releaseNativeApp, type NativeAppRelease } from "./native-app";

const NativeAppDialog = lazy(() => import("./native-app-dialog").then((module) => ({ default: module.NativeAppDialog })));

/**
 * The move to the native app (native-app.ts), mounted on a Mac only and loaded
 * after the first paint: the notice once the backend says this launch moves
 * and the check finds a release, held back while What's new is open so the two
 * sheets never stack.
 */
export function NativeAppMigration({ whatsNewOpen }: { whatsNewOpen: boolean }) {
  const [release, setRelease] = useReducer((_: NativeAppRelease | null, next: NativeAppRelease | null) => next, null);
  useEffect(() => {
    let cancelled = false;
    void nativeMigrationRelease().then((found) => {
      if (found && cancelled) releaseNativeApp(found);
      else if (found) setRelease(found);
    });
    return () => { cancelled = true; };
  }, []);
  if (!release || whatsNewOpen) return null;
  return (
    <Suspense fallback={null}>
      <NativeAppDialog release={release} onClose={() => setRelease(null)} />
    </Suspense>
  );
}
