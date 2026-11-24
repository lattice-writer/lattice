//! Files dragged in from the Finder onto the Agent panel, attached to its
//! composer (`agent-composer-files.ts`): the island takes the drag from
//! WebKit (`Island::accept_file_drops`), so Synara's frame never sees it,
//! and the page hears each phase (`agent:file-drag`). Any file is an
//! attachment here, whatever its format (lattice#1254; the Tauri app still
//! takes only figures and sources), so the page lights the panel for any
//! drag and on the drop asks for its bytes (`read_agent_composer_files`,
//! answered by [`lattice_core::project::read_agent_attachments`]), which
//! it relays into the composer as its "+" menu would, with the count of
//! files past the composer's cap that were never read. The bytes go to the
//! composer alone, never into an editor or the project.
//!
//! The bytes read are only those of the drop that just happened: a page
//! cannot have the app read a path it names.

use gpui::{AppContext as _, Context};
use serde_json::{Value, json};

use super::SynaraSurface;

impl SynaraSurface {
    /// A phase of a Finder drag over the page ([`lattice_webview::IslandEvent::FileDrop`]).
    pub(super) fn file_dragged(&mut self, payload: &Value, cx: &mut Context<Self>) {
        let phase = payload
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("leave");
        let paths: Vec<String> = payload
            .get("paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if phase == "drop" {
            self.dropped = paths.clone();
        }
        self.post(
            "agent:file-drag",
            json!({ "phase": phase, "paths": paths }),
            cx,
        );
    }

    /// `read_agent_composer_files`, for the paths of the last drop alone.
    pub(super) fn read_dropped(&mut self, id: u64, cx: &mut Context<Self>) {
        let dropped = std::mem::take(&mut self.dropped);
        let read =
            cx.background_spawn(
                async move { lattice_core::project::read_agent_attachments(&dropped) },
            );
        cx.spawn(async move |this, cx| {
            let attachments = read.await;
            this.update(cx, |this, cx| {
                this.drop_read = Some(attachments.as_ref().map_err(Clone::clone).map(|read| {
                    read.files
                        .iter()
                        .map(|file| (file.name.clone(), file.mime_type.clone()))
                        .collect()
                }));
                let answer = attachments
                    .and_then(|read| serde_json::to_value(read).map_err(|error| error.to_string()));
                this.respond(id, answer, cx);
            })
            .ok();
        })
        .detach();
    }

    /// The names and types of the files the page's last read of a drop
    /// took, or why it took none, for a scenario.
    pub fn drop_read(&self) -> Option<&Result<Vec<(String, String)>, String>> {
        self.drop_read.as_ref()
    }
}
