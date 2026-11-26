//! A project entered with nothing changed since its last build.
//!
//! Entering a project (opening it, switching back to its tab) builds it, so
//! the PDF and the build's messages are of the files as they are. When the
//! last build succeeded and nothing it read has changed since, latexmk would
//! find nothing to do; running it still costs a process, the build's busy
//! state, and a PDF the viewer loads again although its bytes are the same
//! (lattice#1267). [`current_build`] answers from what that build left
//! instead: latexmk's own record of every file each of its rules read
//! (`.fdb_latexmk`, with the size and MD5 of each), compared with the files
//! now, and the TeX log for the messages.
//!
//! Anything this cannot vouch for builds as before: a failed or cancelled
//! last run, a missing PDF or SyncTeX map, a PDF something else wrote over
//! (`shown_pdf::replaced`), an rc file (whose rules latexmk alone knows), the
//! engine switched since, a preamble format that would be made again, or a
//! record in a format this does not read. The messages are diagnosed as a
//! build diagnoses them, the format's and the PDF fonts' included.

use super::super::aux_files::{self, AuxFiles};
use super::super::build_log::{log_conference_template, pages_written, trim_log};
use super::super::preamble_format::{self, Prepared};
use super::super::{arxiv, lacks_synctex, shown_pdf};
use super::{beside, diagnose};
use crate::latex::BuildResult;
use crate::project;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The result of building `root` now, when its last build of the document a
/// build would compile succeeded and nothing that build read changed since:
/// that build's messages, its PDF and SyncTeX map, marked `unchanged`, with
/// no time spent compiling (`duration_ms` 0). Nothing is typeset; choosing
/// the document may record the compile root in the manifest, as the start
/// of any build does (`choose_document`). `None` when a build has to run
/// to know.
pub fn current_build(
    root: &Path, open_document: Option<&str>, layout: AuxFiles,
) -> Option<BuildResult> {
    if project::has_latexmkrc(root) {
        return None;
    }
    let (document, one_off) = super::super::choose_document(root, open_document).ok()?;
    if project::is_typst_path(&document) {
        return None;
    }
    let manifest = project::read_manifest(root).ok()?;
    let layout = beside::layout_for(root, layout);
    let pdf = root.join(&document).with_extension("pdf");
    if !pdf.is_file() || lacks_synctex(root, &document) || shown_pdf::replaced(root, &document) {
        log::info!("entry builds: {document} has no PDF and SyncTeX map of its own build");
        return None;
    }
    let Ok(database) =
        fs::read_to_string(aux_files::in_layout(root, &document, "fdb_latexmk", layout))
    else {
        log::info!("entry builds: no latexmk record of {document}");
        return None;
    };
    // latexmk runs in the document's folder (`-cd`): its record names files
    // from there.
    let folder = root.join(&document).parent()?.to_path_buf();
    let engine = engine_rule(project::latexmk_engine_arg(&manifest.engine));
    if !inputs_unchanged(&database, &folder, engine) {
        return None;
    }
    // The preamble's packages are read into the format, not by this pass:
    // the format's own record says whether they changed.
    let format = match preamble_format::existing(root, &manifest, &document, &root.join(&document))?
    {
        Prepared::Ready(format) => Some(format),
        _ => None,
    };
    let tex_log = fs::read_to_string(aux_files::in_layout(root, &document, "log", layout)).ok()?;
    // A conference template's PDF has its fonts checked, as after a build;
    // a template in the preamble format is named in the format's log alone.
    let venue = format
        .as_ref()
        .and_then(|format| log_conference_template(&format.log))
        .or_else(|| log_conference_template(&tex_log));
    let bytes = venue.and_then(|_| fs::read(&pdf).ok());
    let (diagnostics, _) = diagnose(root, &document, &tex_log, format.as_ref(), bytes.as_deref());
    if diagnostics.iter().any(|diagnostic| diagnostic.level == "error") {
        return None;
    }
    Some(BuildResult {
        success: true,
        has_pdf: true,
        log: trim_log(&tex_log),
        duration_ms: 0,
        diagnostics,
        outputs: arxiv::written_by_build(root, &document),
        aux_folder: aux_files::relative_folder(&document, layout),
        pages: pages_written(&tex_log),
        venue,
        synctex: Some(true),
        steps: Vec::new(),
        unchanged: true,
        one_off,
        root_document: document,
    })
}

/// The latexmk rule that typesets with `engine_arg` (`-pdf`, `-pdfxe`,
/// `-pdflua`).
fn engine_rule(engine_arg: &str) -> &'static str {
    match engine_arg {
        "-pdfxe" => "xelatex",
        "-pdflua" => "lualatex",
        _ => "pdflatex",
    }
}

/// Whether latexmk's record `database` says its last run of every rule
/// succeeded, one of them typesetting with `engine`, and every file a rule
/// read still holds what it held then. Paths in it are relative to `folder`.
///
/// The record (`# Fdb version 3` or `4`) lists each rule as
/// `["name"] time "source" "dest" "base" time result`, then a line per file
/// it read, `"path" time size md5 "from rule"`, then the files it wrote after
/// `(generated)` and `(rewritten before read)`, by name alone.
fn inputs_unchanged(database: &str, folder: &Path, engine: &str) -> bool {
    let mut lines = database.lines();
    let version = lines.next().and_then(|line| line.strip_prefix("# Fdb version "));
    if !matches!(version.map(str::trim), Some("3" | "4")) {
        return false;
    }
    let mut typeset = false;
    let mut reading = false;
    let mut files = 0;
    for line in lines {
        if let Some(rule) = line.strip_prefix('[') {
            let Some((name, rest)) = rule.split_once(']') else { return false };
            // A rule whose last run failed (or never finished) has something
            // left to do.
            if rest.split_whitespace().last() != Some("0") {
                log::info!("entry builds: latexmk's last run of {name} did not succeed");
                return false;
            }
            typeset |= name.trim_matches('"') == engine;
            reading = true;
            continue;
        }
        let entry = line.trim_start();
        if entry.starts_with('(') {
            reading = false;
            continue;
        }
        if !reading || entry.is_empty() {
            continue;
        }
        let Some((path, fields)) = quoted(entry) else { return false };
        let mut fields = fields.split_whitespace();
        let (Some(time), Some(size), Some(md5)) = (fields.next(), fields.next(), fields.next())
        else {
            return false;
        };
        if !unchanged(&recorded_path(folder, path), time, size, md5) {
            log::info!("entry builds: {path} changed since the last build");
            return false;
        }
        files += 1;
    }
    typeset && files > 0
}

/// Where a path latexmk recorded is: as written when absolute, else under
/// `folder` a component at a time, since a Windows folder (`\\?\D:\…`) takes
/// neither `/` nor `.` as part of a path joined to it.
fn recorded_path(folder: &Path, path: &str) -> PathBuf {
    let recorded = Path::new(path);
    if recorded.is_absolute() || recorded.has_root() {
        return recorded.to_path_buf();
    }
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(folder.to_path_buf(), |path, part| path.join(part))
}

/// The quoted name at the start of `entry`, and what follows it.
fn quoted(entry: &str) -> Option<(&str, &str)> {
    let rest = entry.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((&rest[..end], &rest[end + 1..]))
}

/// Whether the file at `path` is the one latexmk recorded, as latexmk itself
/// judges it: by its MD5, read only when the time or size it was recorded
/// with no longer match. A file that did not exist then (MD5 `0`) must not
/// exist now.
fn unchanged(path: &Path, time: &str, size: &str, md5: &str) -> bool {
    let Ok(metadata) = fs::metadata(path) else { return md5 == "0" };
    if md5 == "0" || !metadata.is_file() {
        return false;
    }
    let recorded_size = size.parse::<u64>().ok();
    if recorded_size.is_some_and(|recorded| recorded != metadata.len()) {
        return false;
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs_f64());
    let same_time = matches!(
        (modified, time.parse::<f64>().ok()),
        (Some(now), Some(then)) if (now - then).abs() < 1e-3
    );
    if recorded_size.is_some() && same_time {
        return true;
    }
    fs::read(path).is_ok_and(|bytes| format!("{:x}", md5::compute(bytes)) == md5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::latex::build::{build, ActiveBuild};
    use crate::latex::BuildSteps;
    use crate::test_support::TempDir;

    fn md5_of(text: &str) -> String {
        format!("{:x}", md5::compute(text))
    }

    /// A record of one pdfLaTeX run over `main.tex` and `ch.tex` as they are
    /// in `root`, with `result` as the rule's last result.
    fn record(root: &Path, result: u8) -> String {
        let mut database = String::from("# Fdb version 4\n");
        database.push_str(&format!(
            "[\"pdflatex\"] 1795561456.1 \"main.tex\" \"main.pdf\" \"main\" 1795561456.5 {result}\n"
        ));
        for name in ["main.tex", "ch.tex"] {
            let text = fs::read_to_string(root.join(name)).unwrap();
            database.push_str(&format!(
                "  \"{name}\" 1700000000.5 {} {} \"\"\n",
                text.len(),
                md5_of(&text)
            ));
        }
        database.push_str("  \"missing.sty\" 0 -1 0 \"\"\n");
        database
            .push_str("  (generated)\n  \"main.aux\"\n  \"main.pdf\"\n  (rewritten before read)\n");
        database
    }

    fn sources() -> TempDir {
        let root = TempDir::new("current-build");
        root.write(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\\input{ch}\\end{document}\n",
        );
        root.write("ch.tex", "A chapter.\n");
        root
    }

    #[test]
    fn a_record_of_the_files_as_they_are_is_current() {
        let root = sources();
        assert!(inputs_unchanged(&record(&root, 0), &root, "pdflatex"));
    }

    #[test]
    fn a_file_rewritten_with_the_same_bytes_is_still_current() {
        let root = sources();
        let database = record(&root, 0);
        // A save that wrote the same text: a new time, the same MD5.
        root.write("ch.tex", "A chapter.\n");
        assert!(inputs_unchanged(&database, &root, "pdflatex"));
    }

    #[test]
    fn an_edited_input_is_not_current() {
        let root = sources();
        let database = record(&root, 0);
        root.write("ch.tex", "A chapter, edited.\n");
        assert!(!inputs_unchanged(&database, &root, "pdflatex"));
        // The same size, other bytes.
        let root = sources();
        let database = record(&root, 0);
        root.write("ch.tex", "B chapter.\n");
        assert!(!inputs_unchanged(&database, &root, "pdflatex"));
    }

    #[test]
    fn a_file_that_appeared_or_went_is_not_current() {
        let root = sources();
        let database = record(&root, 0);
        root.write("missing.sty", "\\ProvidesPackage{missing}\n");
        assert!(!inputs_unchanged(&database, &root, "pdflatex"));
        let root = sources();
        let database = record(&root, 0);
        fs::remove_file(root.join("ch.tex")).unwrap();
        assert!(!inputs_unchanged(&database, &root, "pdflatex"));
    }

    #[test]
    fn a_failed_run_another_engine_or_an_unknown_record_is_not_current() {
        let root = sources();
        assert!(!inputs_unchanged(&record(&root, 12), &root, "pdflatex"));
        assert!(!inputs_unchanged(&record(&root, 0), &root, "xelatex"));
        let future = record(&root, 0).replace("# Fdb version 4", "# Fdb version 9");
        assert!(!inputs_unchanged(&future, &root, "pdflatex"));
        assert!(!inputs_unchanged("", &root, "pdflatex"));
    }

    #[test]
    fn recorded_paths_are_joined_a_component_at_a_time() {
        let folder = Path::new("/project/paper");
        assert_eq!(recorded_path(folder, "./refs.bib"), folder.join("refs.bib"));
        assert_eq!(
            recorded_path(folder, "chapters/intro.tex"),
            folder.join("chapters").join("intro.tex")
        );
        assert_eq!(
            recorded_path(folder, "/usr/share/article.cls"),
            Path::new("/usr/share/article.cls")
        );
    }

    #[test]
    fn the_engine_rule_follows_latexmk_engine_argument() {
        assert_eq!(engine_rule(project::latexmk_engine_arg("pdf")), "pdflatex");
        assert_eq!(engine_rule(project::latexmk_engine_arg("xelatex")), "xelatex");
        assert_eq!(engine_rule(project::latexmk_engine_arg("lualatex")), "lualatex");
    }

    #[test]
    fn a_project_never_built_has_no_current_build() {
        let root = sources();
        project::write_manifest(&root, &project::default_manifest("paper")).unwrap();
        assert!(current_build(&root, None, AuxFiles::Beside).is_none());
    }

    /// Whether this machine can build: the tests that run latexmk return
    /// early where it cannot, instead of being ignored, so they run in every
    /// test run where TeX is installed (the GPUI macOS job, the local gate).
    /// Not on Windows yet: under MiKTeX the entry check still declines a
    /// build that succeeded (the preamble format remade, a pdflatex result
    /// recorded as failed), which #1277 tracks.
    fn tex() -> bool {
        !cfg!(windows)
            && crate::commands::available("latexmk")
            && crate::commands::available("pdflatex")
    }

    /// lattice-core's own build, then the entry: current until a source
    /// changes, and again after the next build.
    #[test]
    fn a_built_project_is_current_until_a_source_changes() {
        if !tex() {
            return;
        }
        for layout in [AuxFiles::Beside, AuxFiles::Hidden] {
            let root = sources();
            project::write_manifest(&root, &project::default_manifest("paper")).unwrap();
            let run = || {
                let steps = BuildSteps::silent();
                build(&root, false, true, &ActiveBuild::default(), None, &steps, layout).unwrap()
            };
            let built = run();
            assert!(built.success && built.has_pdf, "{}", built.log);

            let current = current_build(&root, None, layout).expect("nothing changed");
            assert!(current.success && current.has_pdf && current.unchanged);
            assert_eq!(current.root_document, built.root_document);
            assert_eq!(current.aux_folder, built.aux_folder);
            assert_eq!(current.pages, Some(1));
            assert_eq!(current.duration_ms, 0);
            // The digest a viewer of these bytes shows.
            let bytes = fs::read(root.join("main.pdf")).unwrap();
            assert_eq!(
                crate::latex::built_pdf_digest(&root, &current.root_document),
                Some(crate::review_pdf::digest(&bytes))
            );

            root.write("ch.tex", "A chapter, edited.\n");
            assert!(current_build(&root, None, layout).is_none());
            assert!(run().success);
            assert!(current_build(&root, None, layout).is_some());
            // A PDF written over by something else builds again.
            root.write("main.pdf", "%PDF-1.5 a collaborator's");
            assert!(current_build(&root, None, layout).is_none());
        }
    }

    /// A package the preamble format holds is read by the format alone, so
    /// latexmk's record never lists it: the format's own record does.
    #[test]
    fn a_package_in_the_preamble_format_changed_is_not_current() {
        // The format needs mylatexformat, which a minimal TeX Live (the
        // hosted runner's scheme-basic) may lack; without it every build is
        // plain and there is no format to test.
        let mylatexformat = crate::commands::command("kpsewhich")
            .arg("mylatexformat.ltx")
            .output()
            .is_ok_and(|output| output.status.success());
        if !tex() || !mylatexformat {
            return;
        }
        let root = TempDir::new("current-build-format");
        project::write_manifest(&root, &project::default_manifest("paper")).unwrap();
        root.write(
            "main.tex",
            "\\documentclass{article}\n\\usepackage{macros}\n\\begin{document}\n\\hello\n\\end{document}\n",
        );
        root.write("macros.sty", "\\newcommand{\\hello}{Hello.}\n");
        let steps = BuildSteps::silent();
        let run =
            || build(&root, false, true, &ActiveBuild::default(), None, &steps, AuxFiles::Beside);
        assert!(run().unwrap().success);
        assert!(preamble_format::format_file(&root).is_some_and(|format| format.is_file()));
        assert!(current_build(&root, None, AuxFiles::Beside).is_some());
        root.write("macros.sty", "\\newcommand{\\hello}{Hello, again.}\n");
        assert!(current_build(&root, None, AuxFiles::Beside).is_none());
    }
}
