//! The folder the Agent works in (lattice#1293), and the projects a folder
//! holds.
//!
//! A manuscript that is a project of its own (`ProjectA/Manuscript/`, with
//! its `.research`) can give the Agent the folder around it (`ProjectA/`,
//! with the analysis beside the paper) without that folder becoming a second
//! project: the history, the settings and the builds stay the manuscript's,
//! and only the Agent's working folder widens. The manifest keeps the choice
//! as the levels up from the project (`..`), so it holds wherever the
//! folders move, and only a folder above the project can be named: the
//! project always stays inside what the Agent sees.
//!
//! The home folder, a folder above it and the root of the file system are
//! never given to the Agent, even when the manifest names one: a project
//! straight inside the home folder keeps the Agent in the project.

use super::manifest::{read_manifest, write_manifest};
use crate::models::ProjectManifest;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

/// The most levels above the project the Agent's folder may be, all of
/// which Settings › Agent offers.
pub const MAX_LEVELS: usize = 3;
/// How deep `projects_inside` looks below the folder it is given.
const MAX_DEPTH: usize = 4;
/// How many folders `projects_inside` looks at before it stops, so a folder
/// as large as a home folder answers at once.
const MAX_FOLDERS: usize = 2_000;
/// Folders never searched for projects, besides hidden ones.
const SKIPPED: [&str; 1] = ["node_modules"];

/// The levels up from the project a manifest's `agentFolder` names, `None`
/// for anything but one to `MAX_LEVELS` `..` parts.
fn levels(value: &str) -> Option<usize> {
    let parts: Vec<&str> = value.split('/').collect();
    (parts.len() <= MAX_LEVELS && parts.iter().all(|part| *part == "..")).then_some(parts.len())
}

/// Whether the Agent may be given `folder`, a folder above a project: one
/// that is there, below the home folder and not the root of the file system.
pub fn may_widen_to(folder: &Path) -> bool {
    // Both sides resolved the same way, so a symlinked home or parent folder,
    // or Windows' `\\?\` form of a resolved path, can't pass as below home.
    let Ok(folder) = fs::canonicalize(folder) else {
        return false;
    };
    if folder.parent().is_none() || !folder.is_dir() {
        return false;
    }
    crate::host_paths::home_dir().is_none_or(|home| {
        let home = fs::canonicalize(&home).unwrap_or(home);
        !home.starts_with(&folder)
    })
}

/// The folder the Agent works in for the project at `root`: the folder above
/// it that its manifest names, when that folder may be given to it
/// (`may_widen_to`), else the project's own.
pub fn agent_folder(root: &Path, manifest: &ProjectManifest) -> PathBuf {
    manifest
        .agent_folder
        .as_deref()
        .and_then(levels)
        .and_then(|levels| root.ancestors().nth(levels))
        .filter(|folder| may_widen_to(folder))
        .map_or_else(|| root.to_path_buf(), Path::to_path_buf)
}

/// Give the Agent of the project at `root` the folder `above` it to work
/// in, or its own folder again with `None` (or the project's root).
pub fn set_agent_folder(root: &Path, above: Option<&Path>) -> Result<ProjectManifest, String> {
    let mut manifest = read_manifest(root)?;
    manifest.agent_folder = match above.filter(|folder| *folder != root) {
        None => None,
        Some(folder) => {
            let levels = root
                .ancestors()
                .position(|ancestor| ancestor == folder)
                .filter(|levels| *levels <= MAX_LEVELS)
                .ok_or_else(|| {
                    "The Agent's folder must be a folder above the project.".to_string()
                })?;
            if !may_widen_to(folder) {
                return Err(
                    "The Agent can't be given the home folder or a folder above it.".to_string()
                );
            }
            Some(vec![".."; levels].join("/"))
        }
    };
    write_manifest(root, &manifest)?;
    Ok(manifest)
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

    #[test]
    fn only_folders_above_the_project_are_read_from_the_manifest() {
        assert_eq!(levels(".."), Some(1));
        assert_eq!(levels("../.."), Some(2));
        for refused in ["", ".", "../analysis", "/tmp", "../../../.."] {
            assert_eq!(levels(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_agent_works_in_the_named_folder_above_or_else_the_project() {
        let fixture = Fixture::project("agent-folder");
        let mut manifest = read_manifest(&fixture.root).unwrap();
        assert_eq!(agent_folder(&fixture.root, &manifest), fixture.root);
        manifest.agent_folder = Some("..".to_string());
        assert_eq!(agent_folder(&fixture.root, &manifest), fixture.parent);
        manifest.agent_folder = Some("../analysis".to_string());
        assert_eq!(agent_folder(&fixture.root, &manifest), fixture.root);
        // Never the root of the file system, however many levels it names.
        let levels_to_root = fixture.root.ancestors().count() - 1;
        manifest.agent_folder = Some(vec![".."; levels_to_root].join("/"));
        assert_eq!(agent_folder(&fixture.root, &manifest), fixture.root);
        assert!(!may_widen_to(Path::new("/")));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_home_folder_is_the_home_folder() {
        let Some(home) = crate::host_paths::home_dir() else {
            return;
        };
        let fixture = Fixture::project("agent-folder-link");
        let link = fixture.parent.join("home-link");
        std::os::unix::fs::symlink(&home, &link).unwrap();
        assert!(!may_widen_to(&link));
        assert!(may_widen_to(&fixture.parent) || home.starts_with(&fixture.parent));
    }

    #[test]
    fn the_setting_is_kept_as_levels_and_cleared_again() {
        let fixture = Fixture::project("agent-folder-set");
        let manifest = set_agent_folder(&fixture.root, Some(&fixture.parent)).unwrap();
        assert_eq!(manifest.agent_folder.as_deref(), Some(".."));
        let written = fs::read_to_string(fixture.root.join(".research/project.json")).unwrap();
        assert!(written.contains("\"agentFolder\": \"..\""), "{written}");
        assert!(set_agent_folder(&fixture.root, Some(&fixture.root.join("figures"))).is_err());
        let cleared = set_agent_folder(&fixture.root, None).unwrap();
        assert_eq!(cleared.agent_folder, None);
        let written = fs::read_to_string(fixture.root.join(".research/project.json")).unwrap();
        assert!(!written.contains("agentFolder"), "{written}");
    }

    #[test]
    fn projects_are_found_inside_a_folder_but_not_inside_each_other() {
        let fixture = Fixture::folder("projects-inside", "ProjectA");
        let folder = &fixture.root;
        for project in ["Manuscript", "papers/second", "papers/second/inner", ".hidden/third"] {
            fs::create_dir_all(folder.join(project).join(".research")).unwrap();
        }
        fs::create_dir_all(folder.join("node_modules/pkg/.research")).unwrap();
        fs::create_dir_all(folder.join("a/b/c/d/deep/.research")).unwrap();
        assert_eq!(
            projects_inside(folder, 10),
            vec![folder.join("Manuscript"), folder.join("papers/second")]
        );
        assert_eq!(projects_inside(folder, 1), vec![folder.join("Manuscript")]);
        assert!(projects_inside(&folder.join("analysis"), 10).is_empty());
    }
}
