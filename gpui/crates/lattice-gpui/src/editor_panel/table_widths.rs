//! The column widths and row heights a Markdown file's tables were given in
//! its visual editor (a column's or a row's border dragged or double-clicked
//! there, `table-column-widths.ts` in the engine), kept across launches and
//! never in the file: per project, by the file's project-relative path, a
//! list of saved tables, each with the signature that names it (its column
//! count and header text), the index it was saved at, its columns' widths
//! (none while only rows were sized) and, once a row was sized, a height or
//! null for each row by its index. Which drawn table takes which
//! entry is the page's to decide; the host keeps the list the page last
//! handed over (`md:table-widths`) and gives it back when the file opens
//! (`doc:open`'s `tableWidths`).
//!
//! The file touched last is kept newest, as many files as [`MAX_FILES`] per
//! project and as many projects as [`MAX_ROOTS`]; a file deleted from a
//! project still on disk is forgotten when the widths are next read in. A
//! rename or move, of the file or a folder holding it, carries its widths to
//! the new path, as the editors' places go (`place_memory.rs`).
//!
//! One JSON file in the app's state directory (`markdown-table-widths.json`),
//! beside the editors' places, written 250 ms after the last change or as an
//! editor closes or the app quits; a scripted run keeps them in memory unless
//! `LATTICE_GPUI_STATE_DIR` names a directory.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::place_memory::{state_dir, write_json};

/// How many files' widths a project keeps.
pub const MAX_FILES: usize = 200;
/// How many projects' widths are kept.
pub const MAX_ROOTS: usize = 64;
/// How many tables a file keeps widths for (the page's `MAX_SAVED`).
const MAX_TABLES: usize = 64;
/// The most columns a table's widths are kept for.
const MAX_COLUMNS: usize = 256;
/// The most rows a table's heights are kept for (the page's `MAX_ROWS`).
const MAX_ROWS: usize = 4096;
/// The longest signature kept, in bytes.
const MAX_SIGNATURE: usize = 4096;

const WRITE_AFTER: Duration = Duration::from_millis(250);

/// One table's widths and row heights in CSS pixels, with what names it. A
/// row of no height is at its content's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedTable {
    pub index: u32,
    pub signature: String,
    pub widths: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heights: Vec<Option<f32>>,
}

impl SavedTable {
    fn valid(&self) -> bool {
        let size = |size: &f32| size.is_finite() && *size > 0.;
        self.signature.len() <= MAX_SIGNATURE
            && (!self.widths.is_empty() || self.heights.iter().any(Option::is_some))
            && self.widths.len() <= MAX_COLUMNS
            && self.heights.len() <= MAX_ROWS
            && self.widths.iter().all(size)
            && self.heights.iter().flatten().all(size)
    }
}

/// Every project's files' saved tables, by project-relative path, the most
/// recent last at both levels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableWidths(IndexMap<PathBuf, IndexMap<String, Vec<SavedTable>>>);

impl TableWidths {
    pub fn of(&self, root: &Path, file: &str) -> Option<&[SavedTable]> {
        self.0.get(root)?.get(file).map(Vec::as_slice)
    }

    /// Keep `tables` as `file`'s (none forgets the file); the file and its
    /// project become the newest, and the oldest go past [`MAX_FILES`] and
    /// [`MAX_ROOTS`]. An entry that is not a table's widths is left out on
    /// its own, and past [`MAX_TABLES`] the first entries go. Whether
    /// anything changed.
    pub fn set(&mut self, root: &Path, file: &str, mut tables: Vec<SavedTable>) -> bool {
        tables.retain(SavedTable::valid);
        if tables.len() > MAX_TABLES {
            tables.drain(..tables.len() - MAX_TABLES);
        }
        let mut files = self.0.shift_remove(root).unwrap_or_default();
        let before = files.shift_remove(file);
        let changed = before.as_ref() != Some(&tables) && !(before.is_none() && tables.is_empty());
        if !tables.is_empty() {
            files.insert(file.to_string(), tables);
        }
        while files.len() > MAX_FILES {
            files.shift_remove_index(0);
        }
        if !files.is_empty() {
            self.0.insert(root.to_path_buf(), files);
        }
        while self.0.len() > MAX_ROOTS {
            self.0.shift_remove_index(0);
        }
        changed
    }

    /// Files of `root` were renamed or moved, each to where `moved` says it
    /// went (`None` for one that stayed): their widths go with them, as
    /// recent as they were, over any left at the new path. Whether anything
    /// changed.
    pub fn remap(&mut self, root: &Path, moved: impl Fn(&str) -> Option<String>) -> bool {
        let Some(files) = self.0.get_mut(root) else {
            return false;
        };
        let moves: Vec<_> = files.keys().map(|file| moved(file)).collect();
        if moves.iter().all(Option::is_none) {
            return false;
        }
        let arrived: HashSet<String> = moves.iter().flatten().cloned().collect();
        *files = std::mem::take(files)
            .into_iter()
            .zip(moves)
            .filter_map(|((file, tables), to)| match to {
                Some(to) => Some((to, tables)),
                None => (!arrived.contains(&file)).then_some((file, tables)),
            })
            .collect();
        true
    }

    /// Forget the files `exists` says are gone, in the projects
    /// `root_exists` says are there (a project on a drive not mounted keeps
    /// its own). Whether anything changed.
    pub fn prune(
        &mut self,
        root_exists: impl Fn(&Path) -> bool,
        exists: impl Fn(&Path, &str) -> bool,
    ) -> bool {
        let mut changed = false;
        for (root, files) in &mut self.0 {
            if !root_exists(root) {
                continue;
            }
            let before = files.len();
            files.retain(|file, _| exists(root, file));
            changed |= files.len() != before;
        }
        self.0.retain(|_, files| !files.is_empty());
        changed
    }
}

struct Store {
    file: Option<PathBuf>,
    widths: TableWidths,
    unwritten: bool,
    changes: u64,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| {
        let file = state_dir().map(|dir| dir.join("markdown-table-widths.json"));
        let mut widths: TableWidths = file
            .as_deref()
            .and_then(|file| std::fs::read(file).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let unwritten = widths.prune(Path::is_dir, |root, file| root.join(file).is_file());
        Mutex::new(Store {
            file,
            widths,
            unwritten,
            changes: 0,
        })
    })
}

/// `file`'s saved tables as the page reads them.
pub fn page_widths(root: &Path, file: &str) -> Value {
    store()
        .lock()
        .ok()
        .and_then(|store| store.widths.of(root, file).map(|tables| json!(tables)))
        .unwrap_or_else(|| json!([]))
}

/// The page's `md:table-widths`: `{ tables }`, the file's whole list.
pub fn page_changed(root: &Path, file: &str, payload: &Value, cx: &mut gpui::App) {
    let Some(tables) = payload
        .get("tables")
        .and_then(|tables| serde_json::from_value::<Vec<SavedTable>>(tables.clone()).ok())
    else {
        return;
    };
    update(|all| all.set(root, file, tables), cx);
}

/// Files of `root` were renamed or moved (see [`TableWidths::remap`]).
pub fn paths_moved(root: &Path, moved: impl Fn(&str) -> Option<String>, cx: &mut gpui::App) {
    update(|all| all.remap(root, moved), cx);
}

fn update(change: impl FnOnce(&mut TableWidths) -> bool, cx: &mut gpui::App) {
    let Ok(mut store) = store().lock() else {
        return;
    };
    if !change(&mut store.widths) {
        return;
    }
    store.unwritten = true;
    store.changes += 1;
    let change = store.changes;
    drop(store);
    cx.spawn(async move |cx| {
        cx.background_executor().timer(WRITE_AFTER).await;
        if self::store()
            .lock()
            .is_ok_and(|store| store.changes == change)
        {
            flush();
        }
    })
    .detach();
}

/// Write the widths now, when they changed since they were last written.
pub fn flush() {
    let Ok(mut store) = store().lock() else {
        return;
    };
    if !std::mem::take(&mut store.unwritten) {
        return;
    }
    if let Some(file) = store.file.clone()
        && let Err(error) = write_json(&file, &store.widths)
    {
        log::warn!("could not save the Markdown tables' sizes: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(index: u32, signature: &str, widths: &[f32]) -> SavedTable {
        SavedTable {
            index,
            signature: signature.to_string(),
            widths: widths.to_vec(),
            heights: Vec::new(),
        }
    }

    fn rows(index: u32, signature: &str, heights: &[Option<f32>]) -> SavedTable {
        SavedTable {
            heights: heights.to_vec(),
            ..table(index, signature, &[])
        }
    }

    #[test]
    fn row_heights_are_kept_beside_widths_or_alone_and_read_back_from_json() {
        let root = Path::new("/p");
        let mut all = TableWidths::default();
        let alone = rows(0, "2|a|b", &[None, Some(40.)]);
        let both = SavedTable {
            heights: vec![Some(30.), None],
            ..table(1, "1|c", &[80.])
        };
        all.set(root, "a.md", vec![alone.clone(), both.clone()]);
        assert_eq!(all.of(root, "a.md"), Some([alone, both].as_slice()));
        let json = serde_json::to_string(&all).unwrap();
        assert_eq!(serde_json::from_str::<TableWidths>(&json).unwrap(), all);
        let widths_only = serde_json::to_value(table(0, "1|a", &[80.])).unwrap();
        assert!(
            widths_only.get("heights").is_none(),
            "an entry with no row heights is written as before"
        );
        for bad in [
            rows(2, "1|d", &[None, None]),
            rows(2, "1|d", &[Some(0.)]),
            rows(2, "1|d", &[Some(f32::NAN)]),
            rows(2, "1|d", &vec![Some(20.); MAX_ROWS + 1]),
        ] {
            all.set(root, "b.md", vec![bad]);
            assert_eq!(all.of(root, "b.md"), None);
        }
    }

    #[test]
    fn a_files_list_is_kept_as_handed_over_and_forgotten_when_empty() {
        let root = Path::new("/p");
        let mut all = TableWidths::default();
        let list = vec![table(0, "2|a|b", &[120., 80.]), table(3, "1|c", &[60.])];
        assert!(all.set(root, "notes.md", list.clone()));
        assert!(
            !all.set(root, "notes.md", list.clone()),
            "the same list again changes nothing"
        );
        assert!(all.set(Path::new("/q"), "notes.md", vec![table(0, "1|x", &[50.])]));
        assert_eq!(all.of(root, "notes.md"), Some(list.as_slice()));
        assert!(all.set(root, "notes.md", Vec::new()));
        assert_eq!(all.of(root, "notes.md"), None);
        assert!(!all.set(root, "other.md", Vec::new()), "nothing to forget");
        assert_eq!(
            all.of(Path::new("/q"), "notes.md").unwrap()[0].widths,
            vec![50.]
        );
    }

    #[test]
    fn an_entry_that_is_not_widths_is_left_out_on_its_own() {
        let root = Path::new("/p");
        let mut all = TableWidths::default();
        let good = table(0, "1|a", &[80.]);
        for bad in [
            table(1, "1|b", &[]),
            table(1, "1|b", &[0.]),
            table(1, "1|b", &[-3.]),
            table(1, "1|b", &[f32::NAN]),
            table(1, "1|b", &[1.; MAX_COLUMNS + 1]),
            table(1, &"s".repeat(MAX_SIGNATURE + 1), &[80.]),
        ] {
            all.set(root, "a.md", vec![bad.clone(), good.clone(), bad]);
            assert_eq!(all.of(root, "a.md"), Some([good.clone()].as_slice()));
        }
        assert!(
            !all.set(root, "b.md", vec![table(0, "1|b", &[0.])]),
            "nothing left, nothing kept"
        );
        assert_eq!(all.of(root, "b.md"), None);
        let many: Vec<_> = (0..MAX_TABLES as u32 + 2)
            .map(|index| table(index, &format!("1|{index}"), &[80.]))
            .collect();
        all.set(root, "c.md", many.clone());
        assert_eq!(
            all.of(root, "c.md"),
            Some(&many[2..]),
            "past the cap the first go"
        );
    }

    #[test]
    fn the_file_and_the_project_touched_longest_ago_go_first() {
        let root = Path::new("/p");
        let mut all = TableWidths::default();
        let one = || vec![table(0, "1|a", &[100.])];
        for file in 0..MAX_FILES {
            all.set(root, &format!("{file}.md"), one());
        }
        all.set(root, "0.md", vec![table(0, "1|a", &[90.])]);
        all.set(root, "new.md", one());
        assert!(all.of(root, "0.md").is_some(), "touched again, kept");
        assert_eq!(all.of(root, "1.md"), None);

        for project in 0..MAX_ROOTS {
            all.set(&PathBuf::from(format!("/r{project}")), "a.md", one());
        }
        assert_eq!(all.of(root, "0.md"), None, "the oldest project went");
        all.set(Path::new("/r0"), "a.md", vec![table(0, "1|a", &[70.])]);
        all.set(Path::new("/new"), "a.md", one());
        assert!(
            all.of(Path::new("/r0"), "a.md").is_some(),
            "touched again, kept"
        );
        assert_eq!(all.of(Path::new("/r1"), "a.md"), None);
    }

    #[test]
    fn a_renamed_files_widths_follow_it_and_read_back_from_json() {
        let root = Path::new("/p");
        let mut all = TableWidths::default();
        let moved = vec![table(1, "2|a|b", &[140.5, 60.])];
        all.set(root, "notes/a.md", moved.clone());
        all.set(root, "b.md", vec![table(0, "1|b", &[80.])]);
        all.set(root, "parts/notes/a.md", vec![table(0, "1|c", &[30.])]);
        assert!(all.remap(root, |file| {
            file.strip_prefix("notes/")
                .map(|rest| format!("parts/notes/{rest}"))
        }));
        assert_eq!(
            all.of(root, "parts/notes/a.md"),
            Some(moved.as_slice()),
            "the moved file wins over what was left at its new path"
        );
        assert_eq!(all.of(root, "notes/a.md"), None);
        assert!(!all.remap(root, |_| None), "nothing moved, nothing changed");
        let json = serde_json::to_string(&all).unwrap();
        assert_eq!(serde_json::from_str::<TableWidths>(&json).unwrap(), all);
    }

    #[test]
    fn deleted_files_are_forgotten_but_not_a_missing_projects() {
        let mut all = TableWidths::default();
        let one = || vec![table(0, "1|a", &[100.])];
        all.set(Path::new("/here"), "kept.md", one());
        all.set(Path::new("/here"), "gone.md", one());
        all.set(Path::new("/unmounted"), "a.md", one());
        assert!(all.prune(
            |root| root == Path::new("/here"),
            |_, file| file == "kept.md"
        ));
        assert!(all.of(Path::new("/here"), "kept.md").is_some());
        assert_eq!(all.of(Path::new("/here"), "gone.md"), None);
        assert!(all.of(Path::new("/unmounted"), "a.md").is_some());
        assert!(!all.prune(|_| true, |_, _| true));
    }
}
