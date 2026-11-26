//! Typst documents in a project: which `.typ` a folder opened for the first
//! time builds, and which one a build compiles while a `.typ` is open, the
//! document that includes it when it is a part.

use super::manifest::{default_root_document, normalize_relative, read_manifest, CompileRoot};
use super::paths::safe_path;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Whether `path` is a Typst source, which the native app compiles itself.
pub fn is_typst_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".typ")
}

/// A Typst file has no document class: any of them compiles. The open one
/// builds when no Typst root stands for it: as the default in a project with
/// none, and once (a one-off) beside a LaTeX default, so a mixed project's
/// paper stays what its other builds compile. Under a Typst default it casts
/// no vote, as that document `#include`s its chapters. A part another
/// `.typ` includes builds that document instead ([`including_typst_root`]):
/// compiled alone, a saved chapter replaced the book's PDF with its own pages.
pub(super) fn resolve_typst_root(root: &Path, relative: String) -> Option<CompileRoot> {
    if !safe_path(root, &relative).ok()?.is_file() {
        return None;
    }
    let exists = |path: &str| safe_path(root, path).is_ok_and(|path| path.is_file());
    let default = read_manifest(root)
        .ok()
        .and_then(|manifest| default_root_document(&manifest).map(|document| document.path.clone()))
        .filter(|default| exists(default));
    match default {
        Some(default) if default == relative => {
            Some(CompileRoot { path: relative, one_off: false })
        }
        Some(default) if is_typst_path(&default) => None,
        Some(_) => Some(CompileRoot {
            path: including_typst_root(root, &relative).unwrap_or(relative),
            one_off: true,
        }),
        None => Some(CompileRoot {
            path: including_typst_root(root, &relative).unwrap_or(relative),
            one_off: false,
        }),
    }
}

/// The `.typ` that pulls `part` in, directly or through other parts, and
/// that no other file includes: the document `part` is a piece of. Several
/// such documents (a book and a handout sharing a chapter) go to the
/// shallowest, then the first by name, so a build always picks the same one.
fn including_typst_root(root: &Path, part: &str) -> Option<String> {
    let mut includes: Vec<(String, Vec<String>)> = Vec::new();
    // Every Typst build beside a LaTeX default walks this, live builds
    // included: hidden folders (`.git`, `.research/build`) and
    // `node_modules` hold no part of the document and are not entered.
    let walk = WalkDir::new(root).max_depth(4).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        entry.depth() == 0 || !(name.starts_with('.') || name == "node_modules")
    });
    for entry in walk.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("typ")) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else { continue };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let content = fs::read_to_string(path).unwrap_or_default();
        let included = typst_includes(&relative, &content);
        includes.push((relative, included));
    }
    let includers_of = |file: &str| {
        includes
            .iter()
            .filter(|(_, included)| included.iter().any(|path| path == file))
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>()
    };
    let mut seen = std::collections::HashSet::new();
    let mut pending = includers_of(part);
    let mut tops = Vec::new();
    while let Some(file) = pending.pop() {
        if file == part || !seen.insert(file.clone()) {
            continue;
        }
        let above = includers_of(&file);
        if above.is_empty() {
            tops.push(file);
        }
        pending.extend(above);
    }
    // Parts that include each other in a loop leave no top: any of them
    // still builds more than the part alone.
    if tops.is_empty() {
        tops = seen.into_iter().collect();
    }
    tops.into_iter()
        .min_by(|a, b| a.matches('/').count().cmp(&b.matches('/').count()).then(a.cmp(b)))
}

/// The files a Typst source names in `include "…"` (markup's `#include` or
/// code's bare `include`), relative to the project: a path resolves from the
/// including file's folder, and one starting with `/` from the project root,
/// as Typst resolves them. An include inside a comment names nothing.
fn typst_includes(file: &str, content: &str) -> Vec<String> {
    let folder = file.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut found = Vec::new();
    for line in strip_typst_comments(content).lines() {
        let mut rest = line;
        while let Some(at) = rest.find("include") {
            let before = rest[..at].chars().next_back();
            let after = rest[at + "include".len()..].trim_start();
            rest = &rest[at + "include".len()..];
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                continue;
            }
            let Some(target) = after.strip_prefix('"').and_then(|text| text.split_once('"')) else {
                continue;
            };
            let target = target.0;
            let joined = match target.strip_prefix('/') {
                Some(absolute) => absolute.to_string(),
                None => format!("{folder}/{target}"),
            };
            if let Some(path) = normalize_relative(&joined) {
                found.push(path);
            }
        }
    }
    found
}

/// `content` with its `// …` line comments and `/* … */` block comments (which
/// nest in Typst, and may span lines) blanked out, newlines kept. A `//` inside
/// a quoted string is text; a quote left open ends with its line, as a stray
/// quote in markup is not a string, and outside one `\` escapes the next
/// character, as in markup. Raw text, from a run of backticks to the next run
/// of as many, is blanked too, newlines kept: a glob such as `chapters/*.typ`
/// in a code example opened a comment that never closed and hid every include
/// after it, and an `#include` shown in one includes nothing.
fn strip_typst_comments(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let (mut quoted, mut depth) = (false, 0usize);
    while let Some(c) = chars.next() {
        if c == '\n' {
            quoted = false;
            out.push(c);
            continue;
        }
        if depth > 0 {
            match (c, chars.peek()) {
                ('*', Some('/')) => {
                    chars.next();
                    depth -= 1;
                }
                ('/', Some('*')) => {
                    chars.next();
                    depth += 1;
                }
                _ => {}
            }
            continue;
        }
        if quoted {
            if c == '\\' {
                out.push(c);
                if let Some(&next) = chars.peek().filter(|next| **next != '\n') {
                    chars.next();
                    out.push(next);
                }
                continue;
            }
            quoted = c != '"';
            out.push(c);
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                while chars.peek().is_some_and(|next| *next != '\n') {
                    chars.next();
                }
            }
            ('/', Some('*')) => {
                chars.next();
                depth = 1;
                out.push(' ');
            }
            ('\\', Some(&next)) => {
                chars.next();
                out.push(c);
                out.push(next);
            }
            ('`', _) => {
                let mut fence = 1;
                while chars.next_if_eq(&'`').is_some() {
                    fence += 1;
                }
                out.push(' ');
                if fence == 2 {
                    continue;
                }
                let mut found = 0;
                while found < fence {
                    match chars.next() {
                        Some('`') => found += 1,
                        Some(c) => {
                            found = 0;
                            if c == '\n' {
                                out.push(c);
                            }
                        }
                        None => break,
                    }
                }
            }
            _ => {
                quoted = c == '"';
                out.push(c);
            }
        }
    }
    out
}

/// A folder with no LaTeX document may be a Typst one: its `main.typ`, or
/// the file the others are included into. A mixed folder's LaTeX wins above.
pub(super) fn detect_typst_root(root: &Path) -> Option<String> {
    let mut best: Option<(i32, String)> = None;
    for entry in WalkDir::new(root).max_depth(4).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("typ")) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else { continue };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.split('/').any(|part| part.starts_with('.')) {
            continue;
        }
        let file_name =
            path.file_name().and_then(|value| value.to_str()).unwrap_or("").to_ascii_lowercase();
        let mut score = match file_name.as_str() {
            "main.typ" => 120,
            "paper.typ" | "manuscript.typ" | "thesis.typ" | "report.typ" | "document.typ" => 90,
            _ => 0,
        };
        let content = fs::read_to_string(path).unwrap_or_default();
        // A document sets itself up; a chapter or a template only defines.
        if content.contains("#include") || content.contains("#show:") {
            score += 40;
        }
        if content.contains("#set document") || content.contains("#set page") {
            score += 20;
        }
        score -= relative.matches('/').count() as i32 * 3;
        if best.as_ref().is_none_or(|(best_score, _)| score > *best_score) {
            best = Some((score, relative));
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProjectManifest;
    use crate::project::manifest::{open, resolve_compile_root, set_compile_root, write_manifest};
    use crate::project::test_support::Fixture;

    const ARTICLE: &str = "\\documentclass{article}\n\\begin{document}\nA\n\\end{document}\n";

    fn root_paths(manifest: &ProjectManifest) -> Vec<(&str, bool)> {
        manifest
            .root_documents
            .iter()
            .map(|document| (document.path.as_str(), document.is_default))
            .collect()
    }

    #[test]
    fn a_typst_file_compiles_when_no_typst_root_stands_for_it() {
        let fixture = Fixture::folder("typst-roots", "mixed");
        fixture.write("paper.tex", ARTICLE);
        fixture.write("notes.typ", "= Notes\n");
        fixture.write("chapters/one.typ", "= One\n");
        let opened = open(&fixture.root).unwrap();
        assert_eq!(root_paths(&opened.manifest), vec![("paper.tex", true)]);
        let target = |path: &str| {
            resolve_compile_root(&fixture.root, path).map(|target| (target.path, target.one_off))
        };
        // Beside a LaTeX default, once.
        assert_eq!(target("notes.typ"), Some(("notes.typ".into(), true)));
        // Under a Typst default, the default stands for its chapters.
        set_compile_root(&fixture.root, "notes.typ").unwrap();
        assert_eq!(target("chapters/one.typ"), None);
        assert_eq!(target("notes.typ"), Some(("notes.typ".into(), false)));
        // With no default at all, it becomes the default.
        let mut manifest = read_manifest(&fixture.root).unwrap();
        manifest.root_documents.clear();
        write_manifest(&fixture.root, &manifest).unwrap();
        assert_eq!(target("chapters/one.typ"), Some(("chapters/one.typ".into(), false)));
        assert_eq!(target("missing.typ"), None);
    }

    /// LEO-117: a part pulled in with `#include`, saved while it is in front
    /// and no Typst default stands, builds the document that includes it,
    /// through any number of parts, never the part alone.
    #[test]
    fn an_included_typst_part_builds_the_document_that_includes_it() {
        let fixture = Fixture::folder("typst-parts", "book");
        fixture.write("paper.tex", ARTICLE);
        fixture.write("book.typ", "= Book\n#include \"chapters/one.typ\"\n");
        fixture.write("chapters/one.typ", "= One\n#include \"../parts/a.typ\"\n");
        fixture.write("parts/a.typ", "Part A\n");
        fixture.write("notes.typ", "= Notes\n// #include \"parts/b.typ\"\n");
        fixture.write("parts/b.typ", "Part B\n");
        open(&fixture.root).unwrap();
        let target = |path: &str| {
            resolve_compile_root(&fixture.root, path).map(|target| (target.path, target.one_off))
        };
        // Beside a LaTeX default: the including document, once.
        assert_eq!(target("chapters/one.typ"), Some(("book.typ".into(), true)));
        assert_eq!(target("parts/a.typ"), Some(("book.typ".into(), true)));
        // A commented-out include pulls nothing in.
        assert_eq!(target("parts/b.typ"), Some(("parts/b.typ".into(), true)));
        // With no default at all, the including document becomes it.
        let mut manifest = read_manifest(&fixture.root).unwrap();
        manifest.root_documents.clear();
        write_manifest(&fixture.root, &manifest).unwrap();
        assert_eq!(target("parts/a.typ"), Some(("book.typ".into(), false)));
        assert_eq!(target("book.typ"), Some(("book.typ".into(), false)));
        assert_eq!(
            typst_includes(
                "ch/x.typ",
                "#include \"/top.typ\"\n#let y = include \"y.typ\"\n#my-include \"z.typ\"\n"
            ),
            vec!["top.typ".to_string(), "ch/y.typ".to_string()]
        );
        // Trailing and block comments, nested and over lines, name nothing;
        // a `//` in a string is text.
        let commented = concat!(
            "#include \"a.typ\" // #include \"b.typ\"\n",
            "/* #include \"c.typ\"\n#include \"d.typ\" /* nested */ #include \"e.typ\" */\n",
            "#include \"f.typ\" /* #include \"g.typ\" */ #include \"h.typ\"\n",
            "#link(\"https://example.org\") #include \"i.typ\"\n",
        );
        assert_eq!(typst_includes("x.typ", commented), ["a.typ", "f.typ", "h.typ", "i.typ"]);
        // Raw text is not code: a glob in a fenced block or inline opens no
        // comment, an include shown in one names nothing, and \" is no quote.
        let raw = concat!(
            "```sh\nls chapters/*.typ\n#include \"shown.typ\"\n```\n",
            "#include \"a.typ\"\n",
            "Files under `src/*` and `x /* y`, then \\\" // #include \"c.typ\"\n",
            "#include \"b.typ\"\n",
            "`` /* #include \"d.typ\" */ #include \"e.typ\"\n",
            "```raw /* ````x` #include \"f.typ\"\n",
            "\\` #include \"g.typ\"\n",
        );
        assert_eq!(typst_includes("x.typ", raw), ["a.typ", "b.typ", "e.typ", "f.typ", "g.typ"]);
        // A part only a hidden folder or node_modules includes stands alone.
        fixture.write(".cache/copy.typ", "#include \"../parts/b.typ\"\n");
        fixture.write("node_modules/pkg/lib.typ", "#include \"../../parts/b.typ\"\n");
        assert_eq!(target("parts/b.typ"), Some(("parts/b.typ".into(), false)));
    }
}
