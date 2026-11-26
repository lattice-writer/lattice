//! The folder the Agent works in (lattice#1293), and the projects a folder
//! holds.
//!
//! A manuscript that is a project of its own (`ProjectA/Manuscript/`, with
//! its `.research`) can give the Agent the folder around it (`ProjectA/`,
//! with the analysis beside the paper) without that folder becoming a second
//! project: the history, the settings and the builds stay the manuscript's,
//! and only the Agent's working folder widens. The manifest keeps the choice
//! as the levels up from the project (`..`), and only a folder above the
//! project can be named: the project always stays inside what the Agent sees.
//!
//! The manifest is only half of it. A project can be cloned from someone
//! else, and the Agent writes inside its own folder, so a manifest naming a
//! folder grants nothing by itself: the person's choice (Settings › Agent ›
//! Display, or the parent folder's question) is also kept in the app's own
//! `agent-folders.json`, outside every project, as the project's and the
//! folder's resolved paths, and the Agent is widened only when both agree.
//! A project that moves keeps its Agent in the project until it is chosen
//! again.
//!
//! The home folder, a folder above it, a folder holding the app's own files
//! and a folder fewer than `MIN_COMPONENTS` deep (`/`, `/Volumes`, `/opt`,
//! `D:\\`) are never given to the Agent.

use super::manifest::{read_manifest, write_manifest};
use crate::models::ProjectManifest;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The most levels above the project the Agent's folder may be, all of
/// which Settings › Agent offers.
pub const MAX_LEVELS: usize = 3;
/// How deep `projects_inside` looks below the folder it is given: no deeper
/// than a project can give the Agent that folder.
const MAX_DEPTH: usize = MAX_LEVELS;
/// How many folders `projects_inside` looks at before it stops, so a folder
/// as large as a home folder answers at once.
const MAX_FOLDERS: usize = 2_000;
/// Folders never searched for projects, besides hidden ones.
const SKIPPED: [&str; 1] = ["node_modules"];
/// The fewest named folders a path to the Agent's folder has, once resolved:
/// `/Volumes/Disk/papers`, never `/Volumes/Disk` or `/opt/homebrew`.
const MIN_COMPONENTS: usize = 3;
/// The app-local file of the folders the person gave, in Application Support.
const GRANTS: &str = "agent-folders.json";

/// The levels up from the project a manifest's `agentFolder` names, `None`
/// for anything but one to `MAX_LEVELS` `..` parts.
fn levels(value: &str) -> Option<usize> {
    let parts: Vec<&str> = value.split('/').collect();
    (parts.len() <= MAX_LEVELS && parts.iter().all(|part| *part == "..")).then_some(parts.len())
}

/// A path as the grants keep it: resolved, so a link or Windows' `\\?\` form
/// names one folder one way.
fn key(path: &Path) -> Option<String> {
    fs::canonicalize(path).ok().map(|path| path.to_string_lossy().into_owned())
}

/// `path` resolved as far as it is there: the grants' folder before the
/// first grant creates it.
fn resolved(path: &Path) -> PathBuf {
    path.ancestors()
        .find_map(|there| {
            let rest = path.strip_prefix(there).ok()?;
            fs::canonicalize(there).ok().map(|there| there.join(rest))
        })
        .unwrap_or_else(|| path.to_path_buf())
}

/// Where the rules look: the home folder, and the grants file.
struct Places {
    home: Option<PathBuf>,
    grants: Option<PathBuf>,
}

impl Places {
    fn host() -> Self {
        Self {
            home: crate::host_paths::home_dir(),
            grants: crate::commands::app_support_dir().map(|dir| dir.join(GRANTS)),
        }
    }

    fn allows(&self, folder: &Path) -> bool {
        // Both sides resolved the same way, so a symlinked home or parent
        // folder, or Windows' `\\?\` form of a resolved path, can't pass as
        // below home.
        let Ok(folder) = fs::canonicalize(folder) else {
            return false;
        };
        let named = folder.components().filter(|part| matches!(part, Component::Normal(_))).count();
        if named < MIN_COMPONENTS || !folder.is_dir() {
            return false;
        }
        let Some(home) = &self.home else {
            return false;
        };
        !resolved(home).starts_with(&folder)
            && !self
                .grants
                .as_deref()
                .and_then(Path::parent)
                .is_some_and(|own| resolved(own).starts_with(&folder))
    }

    fn granted(&self) -> BTreeMap<String, String> {
        self.grants
            .as_deref()
            .and_then(|file| fs::read(file).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn agent_folder(&self, root: &Path, manifest: &ProjectManifest) -> PathBuf {
        let named = manifest
            .agent_folder
            .as_deref()
            .and_then(levels)
            .and_then(|levels| root.ancestors().nth(levels));
        let Some(folder) = named else {
            return root.to_path_buf();
        };
        let given = key(root)
            .zip(key(folder))
            .is_some_and(|(root, folder)| self.granted().get(&root) == Some(&folder));
        if given && self.allows(folder) {
            folder.to_path_buf()
        } else {
            root.to_path_buf()
        }
    }

    fn set_agent_folder(
        &self, root: &Path, above: Option<&Path>,
    ) -> Result<ProjectManifest, String> {
        let mut manifest = read_manifest(root)?;
        let above = above.filter(|folder| *folder != root);
        manifest.agent_folder = match above {
            None => None,
            Some(folder) => {
                let levels = root
                    .ancestors()
                    .position(|ancestor| ancestor == folder)
                    .filter(|levels| *levels <= MAX_LEVELS)
                    .ok_or_else(|| {
                        "The Agent's folder must be a folder above the project.".to_string()
                    })?;
                if !self.allows(folder) {
                    return Err("The Agent can't be given the home folder or a folder above it."
                        .to_string());
                }
                Some(vec![".."; levels].join("/"))
            }
        };
        // The grant first: the manifest's write is what reloads the Agent.
        let mut granted = self.granted();
        let project = key(root).ok_or_else(|| format!("{} is not there.", root.display()))?;
        match above.and_then(key) {
            Some(folder) => granted.insert(project, folder),
            None => granted.remove(&project),
        };
        if let Some(file) = &self.grants {
            let write = || -> std::io::Result<()> {
                if let Some(dir) = file.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(file, serde_json::to_vec_pretty(&granted)?)
            };
            write().map_err(|error| error.to_string())?;
        } else if above.is_some() {
            return Err("Lattice can't find its Application Support folder.".to_string());
        }
        write_manifest(root, &manifest)?;
        Ok(manifest)
    }
}

/// Whether the Agent may be given `folder`, a folder above a project: one
/// that is there, deep enough, below the home folder and outside the app's
/// own files.
pub fn may_widen_to(folder: &Path) -> bool {
    Places::host().allows(folder)
}

/// The folder the Agent works in for the project at `root`: the folder above
/// it that its manifest names, when the person gave it here and it may be
/// given (`may_widen_to`), else the project's own.
pub fn agent_folder(root: &Path, manifest: &ProjectManifest) -> PathBuf {
    Places::host().agent_folder(root, manifest)
}

/// Give the Agent of the project at `root` the folder `above` it to work
/// in, or its own folder again with `None` (or the project's root).
pub fn set_agent_folder(root: &Path, above: Option<&Path>) -> Result<ProjectManifest, String> {
    Places::host().set_agent_folder(root, above)
}

/// The Lattice projects (folders holding a `.research`) inside `folder`,
/// nearest first and by name, at most `limit` of them. It looks
/// `MAX_DEPTH` folders down and at `MAX_FOLDERS` folders at most, past no
/// hidden folder, `node_modules` or link, and never inside a project it
/// found.
pub fn projects_inside(folder: &Path, limit: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([(folder.to_path_buf(), 1)]);
    let mut looked = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && !SKIPPED.contains(&name.as_ref())
            })
            .map(|entry| entry.path())
            .collect();
        children.sort();
        for child in children {
            looked += 1;
            if looked > MAX_FOLDERS {
                return found;
            }
            if child.join(".research").is_dir() {
                found.push(child);
                if found.len() >= limit {
                    return found;
                }
            } else if depth < MAX_DEPTH {
                queue.push_back((child, depth + 1));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::test_support::Fixture;

    /// The rules with a home folder and grants of their own, beside the
    /// project's folder as the real ones are.
    fn scratch(elsewhere: &Fixture) -> Places {
        Places {
            home: Some(elsewhere.root.join("home")),
            grants: Some(elsewhere.root.join("support").join(GRANTS)),
        }
    }

    /// A project at `<temp>/lattice-<label>-<id>/ProjectA/paper`, so the
    /// folder above it is deep enough (`MIN_COMPONENTS`) wherever the
    /// temporary folder is: Linux's `/tmp` is a single named folder, so a
    /// project made straight in it has a folder above that is never given.
    fn deep_project(label: &str) -> (Fixture, PathBuf, PathBuf) {
        let folder = Fixture::folder(label, "ProjectA");
        let root = crate::project::create(&folder.root, "paper").unwrap();
        let parent = folder.root.clone();
        (folder, parent, root)
    }

    #[test]
    fn only_folders_above_the_project_are_read_from_the_manifest() {
        assert_eq!(levels(".."), Some(1));
        assert_eq!(levels("../.."), Some(2));
        for refused in ["", ".", "../analysis", "/tmp", "../../../.."] {
            assert_eq!(levels(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_agent_works_in_the_folder_the_person_gave_and_no_other() {
        let (_fixture, parent, root) = deep_project("agent-folder");
        let elsewhere = Fixture::empty("agent-folder-elsewhere");
        let places = scratch(&elsewhere);
        let mut manifest = read_manifest(&root).unwrap();
        assert_eq!(places.agent_folder(&root, &manifest), root);
        // A manifest that names the folder above (a cloned project, or the
        // Agent's own edit) grants nothing.
        manifest.agent_folder = Some("..".to_string());
        assert_eq!(places.agent_folder(&root, &manifest), root);
        let manifest = places.set_agent_folder(&root, Some(&parent)).unwrap();
        assert_eq!(places.agent_folder(&root, &manifest), parent);
        let mut other = manifest.clone();
        other.agent_folder = Some("../analysis".to_string());
        assert_eq!(places.agent_folder(&root, &other), root);
        other.agent_folder = Some("../..".to_string());
        assert_eq!(places.agent_folder(&root, &other), root);
        let cleared = places.set_agent_folder(&root, None).unwrap();
        assert_eq!(places.agent_folder(&root, &manifest), root);
        assert_eq!(cleared.agent_folder, None);
    }

    #[test]
    fn shallow_folders_home_and_the_apps_own_are_never_given() {
        let (_fixture, parent, root) = deep_project("agent-folder-refused");
        let elsewhere = Fixture::empty("agent-folder-elsewhere");
        let places = scratch(&elsewhere);
        for refused in ["/", "/Volumes", "/opt", "/private"] {
            assert!(!places.allows(Path::new(refused)), "{refused}");
        }
        assert!(places.allows(&parent));
        let home = Places { home: Some(root.clone()), ..scratch(&elsewhere) };
        assert!(!home.allows(&parent));
        let own = Places { grants: Some(root.join("support").join(GRANTS)), ..scratch(&elsewhere) };
        assert!(!own.allows(&parent));
        let homeless = Places { home: None, ..scratch(&elsewhere) };
        assert!(!homeless.allows(&parent));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_home_folder_is_the_home_folder() {
        let fixture = Fixture::project("agent-folder-link");
        let elsewhere = Fixture::empty("agent-folder-elsewhere");
        let places = scratch(&elsewhere);
        let home = elsewhere.root.join("home");
        fs::create_dir_all(home.join("papers")).unwrap();
        let link = fixture.parent.join("home-link");
        std::os::unix::fs::symlink(&home, &link).unwrap();
        assert!(!places.allows(&link));
        assert!(places.allows(&home.join("papers")));
    }

    #[test]
    fn the_setting_is_kept_as_levels_and_cleared_again() {
        let (_fixture, parent, root) = deep_project("agent-folder-set");
        let elsewhere = Fixture::empty("agent-folder-elsewhere");
        let places = scratch(&elsewhere);
        let manifest = places.set_agent_folder(&root, Some(&parent)).unwrap();
        assert_eq!(manifest.agent_folder.as_deref(), Some(".."));
        let written = fs::read_to_string(root.join(".research/project.json")).unwrap();
        assert!(written.contains("\"agentFolder\": \"..\""), "{written}");
        assert_eq!(places.granted().len(), 1);
        assert!(places.set_agent_folder(&root, Some(&root.join("figures"))).is_err());
        let cleared = places.set_agent_folder(&root, None).unwrap();
        assert_eq!(cleared.agent_folder, None);
        let written = fs::read_to_string(root.join(".research/project.json")).unwrap();
        assert!(!written.contains("agentFolder"), "{written}");
        assert!(places.granted().is_empty());
    }

    #[test]
    fn projects_are_found_inside_a_folder_but_not_inside_each_other() {
        let fixture = Fixture::folder("projects-inside", "ProjectA");
        let folder = &fixture.root;
        for project in ["Manuscript", "papers/second", "papers/second/inner", ".hidden/third"] {
            fs::create_dir_all(folder.join(project).join(".research")).unwrap();
        }
        fs::create_dir_all(folder.join("node_modules/pkg/.research")).unwrap();
        fs::create_dir_all(folder.join("a/b/c/deep/.research")).unwrap();
        assert_eq!(
            projects_inside(folder, 10),
            vec![folder.join("Manuscript"), folder.join("papers/second")]
        );
        assert_eq!(projects_inside(folder, 1), vec![folder.join("Manuscript")]);
        assert!(projects_inside(&folder.join("analysis"), 10).is_empty());
    }
}
