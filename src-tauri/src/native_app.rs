//! The native Lattice, which from 0.20.0 is Lattice on every Mac.
//!
//! Each release carries the native (GPUI) app for Macs, with its own updater
//! feed (`latest-native.json`) beside this app's, and from 0.20.0 this app's
//! feed (`latest.json`) names the native archive for Macs too, so an older
//! copy updates straight into it. A copy of this version that still starts on
//! a Mac moves itself at launch (`take_native_move`): the page asks
//! `check_native_app`, which finds the native release and leaves it in the
//! window's resources table the way the updater plugin's own `check` does, so
//! the same claim, install and restart path (`app_restart.rs`,
//! `restart_lattice`) puts it in place of this bundle. The native archive
//! unpacks to Lattice.app with this app's bundle identifier, so the updater
//! replaces the bundle at the same path.
//!
//! Both apps share version numbers (0.20.0 is both), so the check accepts any
//! version the feed names: moving is never an "update" in the semver sense.
//! The signature is still checked against the pinned updater key, which is why
//! a rehearsal may point `LATTICE_NATIVE_UPDATER_ENDPOINT` at another feed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{Manager, ResourceId, Url, Webview};
use tauri_plugin_updater::UpdaterExt;

/// Only Macs get the native app (Apple silicon and Intel alike); Windows and
/// Linux keep this one (Linux does not even register the updater plugin).
const SUPPORTED: bool = cfg!(target_os = "macos");

/// Whether this build moves itself to the native app: a release build for a
/// Mac, which release.yml and macos-intel.yml mark with
/// `LATTICE_MIGRATES_TO_NATIVE` at build time, so a development build, the
/// e2e and the perf lab never replace themselves with the native app.
const MIGRATES: bool = SUPPORTED
    && matches!(option_env!("LATTICE_MIGRATES_TO_NATIVE"), Some(value) if !value.is_empty());

/// Set once a window took this launch's move, so only the first one shows it.
static MIGRATION_TAKEN: AtomicBool = AtomicBool::new(false);

/// The native feed, asked in order: the repository's home in the
/// lattice-writer organization, then its address before the move. GitHub
/// redirects the old address only while nobody recreates it, so a copy that
/// asks the new one first does not depend on that. The updater moves on to
/// the next address when one fails or does not answer 2xx, and trusts only
/// the pinned key's signature, never the address.
const NATIVE_FEEDS: [&str; 2] = [
    "https://github.com/lattice-writer/lattice/releases/latest/download/latest-native.json",
    "https://github.com/leo1oel/lattice/releases/latest/download/latest-native.json",
];
const NATIVE_FEED_OVERRIDE: &str = "LATTICE_NATIVE_UPDATER_ENDPOINT";

/// A check that hangs must not leave the offer dialog spinning.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// The native release a check found, held under `rid` for `install_update`.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAppRelease {
    version: String,
    rid: ResourceId,
}

/// The feeds to ask: the release feeds, unless a rehearsal names another.
fn native_feeds(override_endpoint: Option<String>) -> Result<Vec<Url>, String> {
    match override_endpoint.filter(|endpoint| !endpoint.trim().is_empty()) {
        Some(endpoint) => Url::parse(endpoint.trim()).map(|url| vec![url]),
        None => NATIVE_FEEDS.into_iter().map(Url::parse).collect(),
    }
    .map_err(|error| error.to_string())
}

/// Whether the asking window moves Lattice to the native app this launch:
/// true once per launch in a release build on a Mac, to the first window that
/// asks; false everywhere else.
#[tauri::command]
pub fn take_native_move() -> bool {
    MIGRATES && !MIGRATION_TAKEN.swap(true, Ordering::SeqCst)
}

/// The native release to move to, or null off a Mac and when the feed has
/// nothing for this Mac (darwin-aarch64 or darwin-x86_64).
#[tauri::command]
pub async fn check_native_app(webview: Webview) -> Result<Option<NativeAppRelease>, String> {
    if !SUPPORTED {
        return Ok(None);
    }
    let feeds = native_feeds(std::env::var(NATIVE_FEED_OVERRIDE).ok())?;
    let update = webview
        .updater_builder()
        .endpoints(feeds)
        .and_then(|builder| builder.version_comparator(|_, _| true).timeout(CHECK_TIMEOUT).build())
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    Ok(update.map(|update| NativeAppRelease {
        version: update.version.clone(),
        rid: webview.resources_table().add(update),
    }))
}

#[cfg(test)]
mod tests {
    use super::{native_feeds, MIGRATES, NATIVE_FEEDS, SUPPORTED};

    fn addresses(feeds: Vec<tauri::Url>) -> Vec<String> {
        feeds.into_iter().map(String::from).collect()
    }

    #[test]
    fn the_release_feeds_unless_a_rehearsal_names_another() {
        assert_eq!(addresses(native_feeds(None).unwrap()), NATIVE_FEEDS);
        // An empty variable is no override: the shell left it set but blank.
        assert_eq!(addresses(native_feeds(Some("  ".into())).unwrap()), NATIVE_FEEDS);
        assert_eq!(
            addresses(
                native_feeds(Some(" http://127.0.0.1:8123/latest-native.json ".into())).unwrap()
            ),
            ["http://127.0.0.1:8123/latest-native.json"]
        );
        assert!(native_feeds(Some("not a url".into())).is_err());
    }

    #[test]
    fn the_new_home_is_asked_before_the_old_address() {
        assert!(NATIVE_FEEDS[0].starts_with("https://github.com/lattice-writer/lattice/"));
        assert_eq!(NATIVE_FEEDS[1], NATIVE_FEEDS[0].replace("/lattice-writer/", "/leo1oel/"));
    }

    #[test]
    fn only_macs_move_to_the_native_app_and_only_release_builds_by_themselves() {
        assert_eq!(SUPPORTED, cfg!(target_os = "macos"));
        assert_eq!(
            MIGRATES,
            SUPPORTED
                && option_env!("LATTICE_MIGRATES_TO_NATIVE").is_some_and(|value| !value.is_empty())
        );
        // Once per launch at most, to the first window that asks.
        let first = super::take_native_move();
        assert_eq!(first, MIGRATES);
        assert!(!super::take_native_move());
    }
}
