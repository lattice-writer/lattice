//! New manuscript from template… (the native app's Project panel): a venue's
//! or a saved template's files written into a folder of the project that is
//! open, as a repository holding code, data and several manuscripts keeps
//! them (lattice#1259). Unlike New project it makes no project: no manifest,
//! brief or `.research`, so the open project stays the one its settings,
//! history and Agent threads belong to.
//!
//! Nothing already on disk is overwritten. The files that would be are named
//! instead, before anything is written; a file that appears while the others
//! are written (another app, a sync) fails the add, and what it wrote goes
//! again, with the folders it made.

use super::{create, paths, templates, Venue};
use crate::util::err;
use std::collections::BTreeSet;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;

/// What a manuscript is made from: New project's built-in venues and the
/// writer's saved templates.
pub enum ManuscriptTemplate<'a> {
    Venue(Venue),
    /// The saved template `id` in `store`.
    Saved {
        store: &'a Path,
        id: &'a str,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ManuscriptError {
    /// The project-relative files already there, in order.
    Clash(Vec<String>),
    Failed(String),
}

impl From<String> for ManuscriptError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

/// A manuscript written into the project, its paths project-relative.
#[derive(Debug, PartialEq, Eq)]
pub struct AddedManuscript {
    pub files: Vec<String>,
    /// Its compile root: the venue's `main.tex` (`main.typ` for Typst), or a
    /// saved template's chosen root.
    pub main: String,
}

/// Write `template`'s files into `folder` (project-relative, made when it is
/// missing), a venue's main document titled with the folder's name.
pub fn add_manuscript(
    root: &Path, folder: &str, template: ManuscriptTemplate,
) -> Result<AddedManuscript, ManuscriptError> {
    let folder = folder.trim().replace('\\', "/");
    let folder = folder.trim_matches('/');
    paths::validate_user_entry(folder)?;
    if !folder
        .split('/')
        .all(|segment| paths::is_plain_segment(segment) && segment.trim() == segment)
    {
        return Err(ManuscriptError::Failed(
            "Choose a folder name without empty, \".\" or \"..\" parts.".into(),
        ));
    }
    let name = folder.rsplit('/').next().unwrap_or(folder);
    let (main, files) = match template {
        ManuscriptTemplate::Venue(venue) => {
            (venue.main_document().to_string(), create::venue_files(venue, name)?)
        }
        ManuscriptTemplate::Saved { store, id } => templates::saved_files(store, id)?,
    };
    let files: create::Files =
        files.into_iter().map(|(relative, body)| (format!("{folder}/{relative}"), body)).collect();
    let clashes: Vec<String> = files
        .iter()
        .filter(|(relative, _)| fs::symlink_metadata(root.join(relative)).is_ok())
        .map(|(relative, _)| relative.clone())
        .collect();
    if !clashes.is_empty() {
        return Err(ManuscriptError::Clash(clashes));
    }
    // The folders the add makes, to remove again should it fail.
    let made: BTreeSet<String> = files
        .iter()
        .flat_map(|(relative, _)| {
            let parts: Vec<&str> = relative.split('/').collect();
            (1..parts.len()).map(move |end| parts[..end].join("/"))
        })
        .filter(|folder| !root.join(folder).exists())
        .collect();
    let mut written = Vec::new();
    let result = files.iter().try_for_each(|(relative, body)| {
        let path = paths::creation_path(root, relative)?;
        let mut file =
            fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| {
                match error.kind() {
                    ErrorKind::AlreadyExists => ManuscriptError::Clash(vec![relative.clone()]),
                    _ => ManuscriptError::Failed(error.to_string()),
                }
            })?;
        written.push(path);
        file.write_all(body).map_err(err).map_err(ManuscriptError::Failed)
    });
    if let Err(error) = result {
        for path in &written {
            let _ = fs::remove_file(path);
        }
        // Deepest first; a folder something else wrote into stays.
        for folder in made.iter().rev() {
            let _ = fs::remove_dir(root.join(folder));
        }
        return Err(error);
    }
    Ok(AddedManuscript {
        files: files.into_iter().map(|(relative, _)| relative).collect(),
        main: format!("{folder}/{main}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{read_manifest, test_support::Fixture};

    #[test]
    fn a_venue_lands_in_the_folder_and_the_project_is_left_as_it_was() {
        let fixture = Fixture::project("manuscript-venue");
        let before = fs::read(fixture.path(".research/project.json")).unwrap();
        let added = add_manuscript(
            &fixture.root,
            "studies/Manuscript",
            ManuscriptTemplate::Venue(Venue::Icml),
        )
        .unwrap();
        assert_eq!(added.main, "studies/Manuscript/main.tex");
        assert_eq!(
            added.files,
            [
                "studies/Manuscript/main.tex",
                "studies/Manuscript/icml2026.sty",
                "studies/Manuscript/icml2026.bst",
                "studies/Manuscript/references.bib",
            ]
        );
        for file in &added.files {
            assert!(fixture.path(file).is_file(), "{file}");
        }
        let main = fixture.read("studies/Manuscript/main.tex");
        assert!(main.contains("Manuscript") && !main.contains("LATTICE_PROJECT_TITLE"));
        // No project of its own, and the open one's manifest untouched.
        assert!(!fixture.path("studies/Manuscript/.research").exists());
        assert_eq!(fs::read(fixture.path(".research/project.json")).unwrap(), before);
        assert_eq!(read_manifest(&fixture.root).unwrap().root_documents[0].path, "main.tex");
    }

    #[test]
    fn a_typst_manuscript_compiles_from_main_typ() {
        let fixture = Fixture::project("manuscript-typst");
        let added = add_manuscript(&fixture.root, "notes", ManuscriptTemplate::Venue(Venue::Typst))
            .unwrap();
        assert_eq!(added.main, "notes/main.typ");
        assert!(fixture.read("notes/main.typ").contains("#let title = \"notes\""));
    }

    #[test]
    fn existing_files_are_named_and_nothing_is_written() {
        let fixture = Fixture::project("manuscript-clash");
        fixture.write("Manuscript/main.tex", "mine");
        fixture.write("Manuscript/references.bib", "@misc{mine}");
        let refused =
            add_manuscript(&fixture.root, "Manuscript", ManuscriptTemplate::Venue(Venue::Neurips));
        assert_eq!(
            refused,
            Err(ManuscriptError::Clash(vec![
                "Manuscript/main.tex".into(),
                "Manuscript/references.bib".into(),
            ]))
        );
        assert_eq!(fixture.read("Manuscript/main.tex"), "mine");
        assert!(!fixture.path("Manuscript/neurips_2026.sty").exists());
    }

    #[test]
    fn a_folder_outside_the_writers_files_is_refused() {
        let fixture = Fixture::project("manuscript-refused");
        for folder in ["", "/", ".research/papers", "../outside", "a//b", "a/./b", "a/ b"] {
            assert!(
                matches!(
                    add_manuscript(&fixture.root, folder, ManuscriptTemplate::Venue(Venue::Blank)),
                    Err(ManuscriptError::Failed(_))
                ),
                "{folder:?}"
            );
        }
        assert!(!fixture.parent.join("outside").exists());
    }

    #[test]
    fn a_failed_add_takes_back_what_it_wrote() {
        let fixture = Fixture::project("manuscript-rollback");
        // A file named "figures" where the thesis wants a folder.
        fixture.write("thesis/figures", "not a folder");
        let failed =
            add_manuscript(&fixture.root, "thesis", ManuscriptTemplate::Venue(Venue::Ustcthesis));
        assert!(matches!(failed, Err(ManuscriptError::Failed(_))), "{failed:?}");
        let left: Vec<_> = fs::read_dir(fixture.path("thesis"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["figures"]);
    }

    #[test]
    fn a_saved_template_brings_its_files_and_compile_root() {
        let fixture = Fixture::project("manuscript-saved");
        fixture.write("source/paper.tex", "\\documentclass{lab}\n");
        fixture.write("source/lab.cls", "class");
        let store = fixture.parent.join("templates");
        let saved = templates::save(
            &store,
            &fixture.path("source"),
            templates::LocalTemplate {
                id: String::new(),
                name: "Lab".into(),
                source_url: String::new(),
                version: String::new(),
                files: vec!["paper.tex".into(), "lab.cls".into()],
                root: "paper.tex".into(),
                engine: "xelatex".into(),
                preview: None,
            },
        )
        .unwrap();
        let added = add_manuscript(
            &fixture.root,
            "drafts/lab",
            ManuscriptTemplate::Saved { store: &store, id: &saved.id },
        )
        .unwrap();
        assert_eq!(added.main, "drafts/lab/paper.tex");
        assert_eq!(added.files, ["drafts/lab/paper.tex", "drafts/lab/lab.cls"]);
        assert_eq!(fixture.read("drafts/lab/lab.cls"), "class");
    }
}
