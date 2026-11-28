//! Native open/save panels for browser workspaces.
//!
//! The dialog plugin always parents its panels to the invoking WebView. In a
//! browser workspace that WebView is deliberately hidden, so these commands
//! show unparented rfd panels instead. A headless host (`--browser-host
//! --headless`) has no desktop to show one on: the panel would open on the
//! virtual display and the tab would wait for it forever, so it refuses.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserDialogOptions {
    title: Option<String>,
    #[serde(default)]
    filters: Vec<BrowserDialogFilter>,
    default_path: Option<PathBuf>,
    #[serde(default)]
    multiple: bool,
    #[serde(default)]
    directory: bool,
    can_create_directories: Option<bool>,
}

#[derive(Deserialize)]
struct BrowserDialogFilter {
    name: String,
    extensions: Vec<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum BrowserDialogSelection {
    One(Option<String>),
    Many(Option<Vec<String>>),
}

fn browser_dialog_builder(options: &BrowserDialogOptions) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = &options.title {
        dialog = dialog.set_title(title);
    }
    if let Some(default_path) = &options.default_path {
        let names_file = default_path.is_file() || !default_path.exists();
        match (default_path.parent(), default_path.file_name()) {
            (Some(parent), Some(file_name)) if names_file => {
                if parent.components().count() > 0 {
                    dialog = dialog.set_directory(parent);
                }
                dialog = dialog.set_file_name(file_name.to_string_lossy());
            }
            _ => dialog = dialog.set_directory(default_path),
        }
    }
    if let Some(can_create_directories) = options.can_create_directories {
        dialog = dialog.set_can_create_directories(can_create_directories);
    }
    for filter in &options.filters {
        dialog = dialog.add_filter(&filter.name, &filter.extensions);
    }
    dialog
}

fn dialog_path(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

/// The dialog plugin always parents its panels to the invoking WebView. In a
/// browser workspace that WebView is deliberately hidden, so AppKit puts the
/// panel behind the real browser. A synchronous, unparented rfd panel runs as
/// its own modal window and therefore appears in front without exposing the
/// bridge WebView itself.
async fn run_browser_dialog<T: Send + 'static>(
    window: &tauri::WebviewWindow, kind: &str, pick: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    if let Some(refusal) =
        browser_dialog_refusal(window.label(), crate::browser_host::launched_headless())
    {
        return Err(refusal.to_string());
    }
    tauri::async_runtime::spawn_blocking(pick)
        .await
        .map_err(|error| format!("Browser {kind} dialog stopped unexpectedly: {error}"))
}

const NOT_A_BROWSER_WORKSPACE: &str = "Browser dialogs are only available to a browser workspace.";
const NO_DESKTOP_FILE_PICKER: &str =
    "This Lattice runs without a desktop, so it cannot show a file picker on its computer.";

/// Why the window labelled `label` may not open a panel, if it may not.
fn browser_dialog_refusal(label: &str, headless: bool) -> Option<&'static str> {
    if !label.starts_with("browser-") {
        Some(NOT_A_BROWSER_WORKSPACE)
    } else if headless {
        Some(NO_DESKTOP_FILE_PICKER)
    } else {
        None
    }
}

#[tauri::command]
pub(crate) async fn browser_dialog_open(
    window: tauri::WebviewWindow, options: BrowserDialogOptions,
) -> Result<BrowserDialogSelection, String> {
    run_browser_dialog(&window, "open", move || {
        let dialog = browser_dialog_builder(&options);
        if options.multiple {
            let paths = if options.directory { dialog.pick_folders() } else { dialog.pick_files() };
            BrowserDialogSelection::Many(
                paths.map(|paths| paths.into_iter().map(dialog_path).collect()),
            )
        } else {
            let path = if options.directory { dialog.pick_folder() } else { dialog.pick_file() };
            BrowserDialogSelection::One(path.map(dialog_path))
        }
    })
    .await
}

#[tauri::command]
pub(crate) async fn browser_dialog_save(
    window: tauri::WebviewWindow, options: BrowserDialogOptions,
) -> Result<Option<String>, String> {
    run_browser_dialog(&window, "save", move || {
        browser_dialog_builder(&options).save_file().map(dialog_path)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_headless_host_refuses_file_pickers_and_only_browser_workspaces_ask() {
        assert_eq!(browser_dialog_refusal("browser-1", false), None);
        assert_eq!(browser_dialog_refusal("browser-1", true), Some(NO_DESKTOP_FILE_PICKER));
        assert_eq!(browser_dialog_refusal("project-1", false), Some(NOT_A_BROWSER_WORKSPACE));
        assert_eq!(browser_dialog_refusal("project-1", true), Some(NOT_A_BROWSER_WORKSPACE));
    }
}
