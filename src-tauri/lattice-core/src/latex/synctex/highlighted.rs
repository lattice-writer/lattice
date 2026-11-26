//! The source line of code a package typeset from a file it generated.
//!
//! minted has Pygments write each code block, highlighted, to a file in its
//! cache folder (`_minted/<hash>.highlight.minted`, minted 2.x's
//! `_minted-<job>/<hash>.pygtex`) and inputs that file in place of the block.
//! SyncTeX records the code there, so `synctex edit` on a code line names the
//! generated file, and nothing the writer can edit: TeX recorded no line of
//! the block itself. The block is found again by its text instead.

use super::{input_names, records, Inputs};
use crate::util;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the writer wrote what `input` (canonical), a file in a package's
/// cache folder, typeset at its line `line` and the point `(x, y)` of `page`.
/// For minted's highlighted code, the line of the code block in the
/// project's `.tex` sources; for any other generated file (svg-inkscape's
/// `.pdf_tex`), or code no block holds, the nearest place on the page TeX
/// recorded a source of the project at. `None` for an input outside the
/// cache folders (a `cachedir=` of another name is not recognised).
pub(super) fn source(
    synctex: &str, inputs: &Inputs, root: &Path, input: &Path, line: u32, page: u32,
    point: (f64, f64),
) -> Option<(PathBuf, u32)> {
    let generated = |path: &Path| {
        path.strip_prefix(root).is_ok_and(|relative| {
            util::in_package_cache_folder(&relative.to_string_lossy().replace('\\', "/"))
        })
    };
    if !generated(input) {
        return None;
    }
    let project_source = |name: &str| {
        let path = inputs.path(name).canonicalize().ok()?;
        (path.starts_with(root) && !generated(&path)).then_some(path)
    };
    let mut seen = HashSet::new();
    let extension = |path: &Path, wanted: &[&str]| {
        path.extension().is_some_and(|extension| {
            wanted.iter().any(|wanted| extension.eq_ignore_ascii_case(wanted))
        })
    };
    // Only what can hold a block or declare one: not the `.aux`, `.bbl` or
    // figures TeX also read.
    let (sources, packages): (Vec<_>, Vec<_>) = input_names(synctex)
        .filter_map(project_source)
        .filter(|path| extension(path, &["tex", "sty", "cls"]) && seen.insert(path.clone()))
        .filter_map(|path| Some((path.clone(), fs::read_to_string(&path).ok()?)))
        .partition(|(path, _)| extension(path, &["tex"]));
    // A package of the project's may declare the environments too.
    let packages: Vec<&str> = packages.iter().map(|(_, text)| text.as_str()).collect();
    let blocks = fs::read_to_string(input)
        .map(|highlighted| blocks(&highlighted, line, sources.iter().cloned(), &packages))
        .unwrap_or_default();
    if blocks.is_empty() {
        let (name, line, _) =
            records::source_at(synctex, page, point, |name| project_source(name).is_some())?;
        return Some((project_source(&name)?, line));
    }
    // minted names its file by the code, so blocks of the same code share
    // it, and TeX opens it once for each, declaring it again under a new tag:
    // the block clicked is the one TeX read that time. Nothing typeset around
    // it tells reliably (a float, a page's header and footer, two blocks in a
    // row), but the order TeX declared the file in does.
    let is_input = |name: &str| inputs.path(name).canonicalize().is_ok_and(|path| path == input);
    let time = records::source_at(synctex, page, point, is_input).map_or(0, |(.., tag)| {
        input_tags(synctex).filter(|(other, name)| *other < tag && is_input(name)).count()
    });
    let ordered = reading_order(&sources, &blocks);
    let block = ordered.get(time).or(ordered.first())?;
    Some((block.path.clone(), block.line))
}

/// Every input SyncTeX declares: its tag and its name, as TeX opened it.
fn input_tags(synctex: &str) -> impl Iterator<Item = (u32, &str)> {
    synctex.lines().filter_map(|record| {
        let (tag, name) = record.strip_prefix("Input:")?.split_once(':')?;
        Some((tag.parse().ok()?, name))
    })
}

/// `blocks` in the order TeX reads them: down the first of `sources` (the
/// document TeX opened first), into each source it `\input`s, `\include`s
/// or `\subfile`s where it does, then any block that leaves unread (a
/// source reached another way) in the order of `sources`.
fn reading_order<'a>(
    sources: &'a [(PathBuf, String)], blocks: &'a [Block<PathBuf>],
) -> Vec<&'a Block<PathBuf>> {
    let mut ordered = Vec::new();
    if let Some((document, _)) = sources.first() {
        let folder = document.parent().unwrap_or(document);
        let mut reading = Reading { folder, sources, blocks, open: Vec::new(), reads: 0 };
        reading.read(document, &mut ordered);
    }
    for block in blocks {
        if !ordered.iter().any(|seen| std::ptr::eq(*seen, block)) {
            ordered.push(block);
        }
    }
    ordered
}

/// Reading the sources from the document down, as TeX does.
struct Reading<'a> {
    /// The document's folder, which TeX names an included file relative to.
    folder: &'a Path,
    sources: &'a [(PathBuf, String)],
    blocks: &'a [Block<PathBuf>],
    /// The sources being read, so a file that includes itself ends.
    open: Vec<PathBuf>,
    /// How many sources were read: a chain of files that each include the
    /// next one twice doubles it at every step.
    reads: usize,
}

impl<'a> Reading<'a> {
    /// Past this many reads the rest of the blocks keep the sources' order.
    const MOST_READS: usize = 1000;

    /// Add `path`'s blocks to `ordered` line by line, reading each source it
    /// includes where it does.
    fn read(&mut self, path: &Path, ordered: &mut Vec<&'a Block<PathBuf>>) {
        let Some((_, text)) = self.sources.iter().find(|(source, _)| source == path) else {
            return;
        };
        if self.reads >= Self::MOST_READS || self.open.iter().any(|open| open == path) {
            return;
        }
        self.reads += 1;
        self.open.push(path.to_path_buf());
        for (index, line) in text.lines().enumerate() {
            let number = index as u32 + 1;
            let blocks = self.blocks.iter();
            ordered.extend(blocks.filter(|block| block.path == path && block.start == number));
            let code = uncommented(line);
            for command in ["\\input{", "\\include{", "\\subfile{"] {
                for rest in code.split(command).skip(1) {
                    let name = rest.split('}').next().unwrap_or_default().trim();
                    if let Some(included) = self.source(name) {
                        self.read(&included, ordered);
                    }
                }
            }
        }
        self.open.pop();
    }

    /// The source `\input{name}` reads: `name.tex`, else `name` itself
    /// (`ch.v2` is `ch.v2.tex`), when it is one of the project's.
    fn source(&self, name: &str) -> Option<PathBuf> {
        let named = self.folder.join(name);
        let mut with_tex = named.clone().into_os_string();
        with_tex.push(".tex");
        [PathBuf::from(with_tex), named]
            .into_iter()
            .filter_map(|candidate| candidate.canonicalize().ok())
            .find(|candidate| self.sources.iter().any(|(source, _)| source == candidate))
    }
}

/// A minted environment holding a block's code: where its code starts, and
/// the line of the clicked code in it (1-based).
#[derive(Debug, PartialEq)]
struct Block<P> {
    path: P,
    start: u32,
    line: u32,
}

/// Every minted environment in `sources` (path and text, in order) holding
/// the code of `highlighted`, a minted cache file, with its line `line`'s
/// place in it. Empty when none does: an `\inputminted` file, code `gobble`
/// changed, or a source edited since the build. `packages` (the project's
/// `.sty` and `.cls`) may declare minted environments too.
fn blocks<P: Clone>(
    highlighted: &str, line: u32, sources: impl IntoIterator<Item = (P, String)>, packages: &[&str],
) -> Vec<Block<P>> {
    let lines: Vec<&str> = highlighted.lines().collect();
    // Pygments wraps the block in a verbatim environment (`Verbatim`, minted
    // 3's `MintedVerbatim`) on lines of their own.
    let begin = usize::from(lines.first().is_some_and(|text| text.starts_with("\\begin{")));
    let end =
        lines.len() - usize::from(lines.last().is_some_and(|text| text.starts_with("\\end{")));
    let Some(code) = lines.get(begin..end.max(begin)) else { return Vec::new() };
    let code: Vec<String> = code.iter().map(|text| highlighted_text(text)).collect();
    // Code with no letter or digit would match too much.
    if !code.iter().any(|text| text.chars().any(char::is_alphanumeric)) {
        return Vec::new();
    }
    let offset = (line as usize).saturating_sub(begin + 1).min(code.len() - 1);
    let sources: Vec<(P, String)> = sources.into_iter().collect();
    let environments = minted_environments(
        sources.iter().map(|(_, text)| text.as_str()).chain(packages.iter().copied()),
    );
    let mut found = Vec::new();
    for (path, source) in sources {
        let source: Vec<&str> = source.lines().collect();
        let text: Vec<String> = source.iter().map(|text| code_text(text)).collect();
        for (start, run) in text.windows(code.len()).enumerate() {
            // The block's whole code, not the start of a longer block's: the
            // environment ends after it (past any blank lines Pygments dropped).
            let closes = || {
                let after =
                    source[start + code.len()..].iter().find(|text| !text.trim().is_empty());
                after.is_some_and(|text| text.trim_start().starts_with("\\end{"))
            };
            if run == code.as_slice() && opens_minted(&source[..start], &environments) && closes() {
                let start = start as u32 + 1;
                found.push(Block { path: path.clone(), start, line: start + offset as u32 });
            }
        }
    }
    found
}

/// The minted environments `sources` can open: `minted`, and each one a
/// `\newminted` among them declares (`\newminted{python}{…}` makes
/// `pythoncode`, `\newminted[name]{python}{…}` makes `name`), with its
/// starred form.
fn minted_environments<'a>(sources: impl Iterator<Item = &'a str>) -> HashSet<String> {
    let mut names = HashSet::from(["minted".to_string()]);
    let mut add = |name: &str| {
        let name = name.trim();
        if !name.is_empty() {
            names.insert(format!("{name}*"));
            names.insert(name.to_string());
        }
    };
    for source in sources {
        // A declaration may run over lines, but not into a comment.
        let source: String = source.lines().map(uncommented).collect::<Vec<_>>().join("\n");
        for rest in source.split("\\newminted").skip(1) {
            let rest = rest.trim_start();
            match rest.strip_prefix('[') {
                Some(rest) => add(rest.split(']').next().unwrap_or_default()),
                None => {
                    let language = rest.strip_prefix('{').and_then(|rest| rest.split('}').next());
                    if let Some(language) = language {
                        add(&format!("{}code", language.trim()));
                    }
                }
            }
        }
        // An environment of the document's own that opens one around its
        // body: `\newenvironment{code}{\begin{minted}{python}}{\end{minted}}`.
        for command in [
            "\\newenvironment",
            "\\renewenvironment",
            "\\NewDocumentEnvironment",
            "\\RenewDocumentEnvironment",
        ] {
            for rest in source.split(command).skip(1) {
                let rest = rest.strip_prefix('*').unwrap_or(rest);
                let Some((name, definition)) =
                    rest.trim_start().strip_prefix('{').and_then(|rest| rest.split_once('}'))
                else {
                    continue;
                };
                // Only its own begin and end code (and a document command's
                // argument specification): never what follows it.
                let parts = if command.contains("Document") { 3 } else { 2 };
                if groups(definition, parts).iter().any(|group| group.contains("\\begin{minted}")) {
                    add(name);
                }
            }
        }
    }
    names
}

/// The first `count` brace groups of `text`, past whitespace and the
/// `[…]` groups before them (an environment's argument count and default).
fn groups(text: &str, count: usize) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while found.len() < count {
        rest = rest.trim_start();
        let (open, close) = match rest.chars().next() {
            Some('{') => ('{', '}'),
            Some('[') => ('[', ']'),
            _ => break,
        };
        let mut depth = 0;
        let Some(end) = rest.char_indices().find_map(|(index, character)| {
            depth += i32::from(character == open) - i32::from(character == close);
            (depth == 0).then_some(index)
        }) else {
            break;
        };
        if open == '{' {
            found.push(&rest[1..end]);
        }
        rest = &rest[end + 1..];
    }
    found
}

/// Whether the source lines `above` a run of code end by opening one of
/// `environments` on them: its `\begin{…}` and options, which may run over
/// several lines (`\begin{minted}[` … `]{python}`). A `verbatim`, an
/// `lstlisting` or a comment holding the same code is not where it was
/// highlighted from.
fn opens_minted(above: &[&str], environments: &HashSet<String>) -> bool {
    // The options of one environment fit in a few lines.
    const OPENING_LINES: usize = 12;
    let opening: Vec<&str> = above
        .iter()
        .rev()
        .take(OPENING_LINES)
        // A whole line of comment is one inside the options, or a line of
        // code (`% setup` in MATLAB): never a blank line.
        .map(|line| match uncommented(line) {
            text if text.trim().is_empty() && !line.trim().is_empty() => "\u{1}",
            text => text,
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let opening = opening.join("\n");
    let Some((_, rest)) = opening.rsplit_once("\\begin{") else { return false };
    let Some((name, arguments)) = rest.split_once('}') else { return false };
    // Nothing but the environment's own options and arguments after it.
    environments.contains(name.trim()) && only_groups(arguments)
}

/// Whether `text` is nothing but whitespace and balanced `[…]` and `{…}`
/// groups: an environment's options and arguments, never a line of code
/// (`d = {}`) or more of a macro (`\newcommand\x{\begin{minted}{c}}`).
fn only_groups(text: &str) -> bool {
    let mut closing = Vec::new();
    // A new line may go on only inside a group (options over lines): one
    // that starts after every group closed is code, even all brackets (`[]`).
    let mut line_ended = false;
    for character in text.chars() {
        if line_ended && closing.is_empty() && !character.is_whitespace() {
            return false;
        }
        line_ended |= character == '\n' && closing.is_empty();
        match character {
            '[' | '{' if closing.is_empty() => {
                closing.push(if character == '[' { ']' } else { '}' })
            }
            '{' => closing.push('}'),
            ']' | '}' if closing.last() == Some(&character) => {
                closing.pop();
            }
            ']' | '}' if closing.is_empty() => return false,
            _ if closing.is_empty() && !character.is_whitespace() => return false,
            _ => {}
        }
    }
    closing.is_empty()
}

/// `line` up to its comment: the first `%` that no backslash escapes.
fn uncommented(line: &str) -> &str {
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match character {
            '%' if !escaped => return &line[..index],
            '\\' => escaped = !escaped,
            _ => escaped = false,
        }
    }
    line
}

/// A line of code reduced to what survives highlighting unchanged: its
/// letters and digits, as Pygments escapes punctuation and spaces
/// (`\PYGZob{}` for `{`). A line of punctuation alone (`}`) is `\u{1}`, so
/// it matches no blank line.
fn code_text(line: &str) -> String {
    let text: String = line.chars().filter(|character| character.is_alphanumeric()).collect();
    if text.is_empty() && !line.trim().is_empty() {
        return "\u{1}".to_string();
    }
    text
}

/// A line Pygments highlighted, reduced as `code_text` reduces its source,
/// without its markup: `\PYG{k}{def}` (a token and its style) is `def`, and
/// an escape such as `\PYGZbs{}` (a backslash) is punctuation. Any other
/// command came from the code itself (an `escapeinside` escape), so it counts
/// as it is written.
fn highlighted_text(line: &str) -> String {
    let mut text = String::new();
    let mut punctuation = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            if character.is_alphanumeric() {
                text.push(character);
            }
            // Braces are the markup's own; Pygments escapes the code's.
            punctuation |= !character.is_whitespace() && !matches!(character, '{' | '}');
            continue;
        }
        let mut name = String::new();
        while let Some(letter) = characters.next_if(char::is_ascii_alphabetic) {
            name.push(letter);
        }
        if !name.starts_with("PYG") {
            text.push_str(&name);
            punctuation = true;
        } else if name.starts_with("PYGZ") {
            punctuation = true;
        } else if characters.next_if_eq(&'{').is_some() {
            // The token's style (`k`, `n+nf`) is markup, not code.
            characters.by_ref().find(|character| *character == '}');
        }
    }
    if text.is_empty() && punctuation {
        return "\u{1}".to_string();
    }
    text
}

#[cfg(test)]
mod tests;
