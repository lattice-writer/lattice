//! The rest of the paper a revision reads beside its local window
//! ([`super::context`]), for a manuscript whose sections are files of their
//! own that the main document `\input`s, `\include`s or `\subfile`s
//! (lattice#1379). The window shows only the passage's own file, so the
//! agent missed the paper's macros and notation, and near a file's first or
//! last lines it missed the section it continues or leads into.
//!
//! [`paper`] adds, within fixed budgets and from nothing but the project's
//! files (the same files always give the same prompt):
//!
//! - the main document's preamble, with the files it `\input`s and the
//!   project's own packages it loads read in place, its blank and
//!   comment-only lines dropped: up to [`PREAMBLE_LENGTH`];
//! - when the window reaches the file's start, the text TeX reads just
//!   before the file, across however many files that spans; when it reaches
//!   the file's end, the text just after: up to [`NEIGHBOUR_LENGTH`] each.
//!
//! The agent still reads no files and runs no tools: everything it sees is
//! in the prompt. Lengths are UTF-16 code units, as the window counts them;
//! at most 7,000 units in all, about 2,000 tokens.

use super::{reach_back, reach_forward};

/// The most of the preamble the agent reads, in UTF-16 code units.
pub const PREAMBLE_LENGTH: usize = 4_000;

/// The most of the paper either side of the file the agent reads, in UTF-16
/// code units.
pub const NEIGHBOUR_LENGTH: usize = 1_500;

/// Deep enough for any real manuscript; a cycle of includes stops at once.
const MAX_DEPTH: usize = 16;

/// The most files one walk reads, however the paper is split.
const MAX_FILES: usize = 200;

const BEGIN_DOCUMENT: &str = "\\begin{document}";
const END_DOCUMENT: &str = "\\end{document}";

/// Text from one of the paper's files, by its project-relative path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excerpt {
    pub path: String,
    pub text: String,
}

/// What the agent reads from the paper's other files: see [`paper`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paper {
    pub preamble: Option<Excerpt>,
    /// The text just before the passage's file, in reading order.
    pub preceding: Vec<Excerpt>,
    /// The text just after it, in reading order.
    pub following: Vec<Excerpt>,
}

impl Paper {
    pub fn is_empty(&self) -> bool {
        self.preamble.is_none() && self.preceding.is_empty() && self.following.is_empty()
    }
}

/// The paper around `[from, to)` of `source`, the live text of the file at
/// `path`, as [`super::context`] takes them. `main` is the main document;
/// `read` gives a project file's text by its project-relative path, `None`
/// when there is no such file. Empty without a main document that has a
/// `\begin{document}`.
pub fn paper(
    main: Option<&str>,
    path: &str,
    source: &str,
    from: usize,
    to: usize,
    read: impl Fn(&str) -> Option<String>,
) -> Paper {
    let Some(main) = main else {
        return Paper::default();
    };
    let main_text = if main == path {
        source.to_string()
    } else {
        match read(main) {
            Some(text) => text,
            None => return Paper::default(),
        }
    };
    let blank = blank_comments(&main_text);
    let Some(body) = blank.find(BEGIN_DOCUMENT) else {
        return Paper::default();
    };
    let mut walk = Walk {
        read: &read,
        current: path,
        files: 0,
        segments: Vec::new(),
        reached: false,
    };
    // In the main document itself, a window that reaches the preamble
    // already shows the part of it the passage sits beside.
    let preamble = if main == path && reach_back(source, from) <= body {
        None
    } else {
        walk.walk(
            main,
            &main_text[..body],
            0,
            &mut vec![main.to_string()],
            true,
        );
        let text = head(&tidy(&walk.text(), false), PREAMBLE_LENGTH);
        (!text.is_empty()).then(|| Excerpt {
            path: main.to_string(),
            text,
        })
    };
    let near_start = reach_back(source, from) == 0;
    let near_end = reach_forward(source, to) == source.len();
    if main == path || !(near_start || near_end) {
        return Paper {
            preamble,
            ..Paper::default()
        };
    }
    walk.segments.clear();
    walk.files = 0;
    walk.reached = false;
    let start = body + BEGIN_DOCUMENT.len();
    let end = blank[start..]
        .find(END_DOCUMENT)
        .map_or(main_text.len(), |at| start + at);
    walk.walk(
        main,
        &main_text[start..end],
        0,
        &mut vec![main.to_string()],
        false,
    );
    let Some(at) = walk
        .segments
        .iter()
        .position(|segment| matches!(segment, Segment::Current))
    else {
        return Paper {
            preamble,
            ..Paper::default()
        };
    };
    let preceding = if near_start {
        let mut excerpts = gather(walk.segments[..at].iter().rev(), tail);
        excerpts.reverse();
        excerpts
    } else {
        Vec::new()
    };
    let following = if near_end {
        gather(walk.segments[at + 1..].iter(), head)
    } else {
        Vec::new()
    };
    Paper {
        preamble,
        preceding,
        following,
    }
}

enum Segment {
    Text {
        path: String,
        text: String,
    },
    /// Where TeX reads the passage's file.
    Current,
}

/// The paper's text in the order TeX reads it, each include read in its
/// place.
struct Walk<'a, R> {
    read: &'a R,
    current: &'a str,
    files: usize,
    segments: Vec<Segment>,
    reached: bool,
}

impl<R: Fn(&str) -> Option<String>> Walk<'_, R> {
    /// Read `text`, from the file at `path`, into the segments, its comments
    /// left out. With
    /// `packages` (the preamble), the project's own packages a
    /// `\usepackage` loads are read after it too.
    fn walk(
        &mut self,
        path: &str,
        text: &str,
        depth: usize,
        stack: &mut Vec<String>,
        packages: bool,
    ) {
        let blank = blank_comments(text);
        let mut cursor = 0;
        for command in commands(&blank, packages) {
            // A package's own line stays, for the packages it names that are
            // not the project's; an include is replaced by what it reads.
            let kept = if command.package {
                command.end
            } else {
                command.start
            };
            self.push(path, &blank[cursor..kept]);
            cursor = command.end;
            for target in &command.targets {
                self.include(path, target, &command, depth, stack, packages);
            }
        }
        self.push(path, &blank[cursor..]);
    }

    fn include(
        &mut self,
        from: &str,
        target: &str,
        command: &Command,
        depth: usize,
        stack: &mut Vec<String>,
        packages: bool,
    ) {
        let extension = if command.package { "sty" } else { "tex" };
        for candidate in candidates(from, target, extension) {
            if candidate == self.current {
                if !self.reached {
                    self.reached = true;
                    self.segments.push(Segment::Current);
                }
                return;
            }
            if depth >= MAX_DEPTH || self.files >= MAX_FILES || stack.contains(&candidate) {
                return;
            }
            let Some(text) = (self.read)(&candidate) else {
                continue;
            };
            self.files += 1;
            // What a file reads starts on a line of its own.
            self.push(from, "\n");
            stack.push(candidate.clone());
            self.walk(&candidate, document_body(&text), depth + 1, stack, packages);
            stack.pop();
            return;
        }
    }

    fn push(&mut self, path: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(Segment::Text {
            path: last,
            text: joined,
        }) = self.segments.last_mut()
            && last == path
        {
            joined.push_str(text);
            return;
        }
        self.segments.push(Segment::Text {
            path: path.to_string(),
            text: text.to_string(),
        });
    }

    fn text(&self) -> String {
        self.segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Text { text, .. } => Some(text.as_str()),
                Segment::Current => None,
            })
            .collect()
    }
}

/// Excerpts from `segments`, nearest the passage's file first, until
/// [`NEIGHBOUR_LENGTH`] is spent; `cut` keeps a segment's end nearest the
/// file.
fn gather<'a>(
    segments: impl Iterator<Item = &'a Segment>,
    cut: fn(&str, usize) -> String,
) -> Vec<Excerpt> {
    let mut left = NEIGHBOUR_LENGTH;
    let mut excerpts = Vec::new();
    for segment in segments {
        let Segment::Text { path, text } = segment else {
            continue;
        };
        let tidied = tidy(text, true);
        let trimmed = tidied.trim();
        if trimmed.is_empty() {
            continue;
        }
        let text = cut(trimmed, left);
        let units = text.encode_utf16().count();
        let whole = units == trimmed.encode_utf16().count();
        if !text.is_empty() {
            excerpts.push(Excerpt {
                path: path.clone(),
                text,
            });
        }
        if !whole || units >= left {
            break;
        }
        left -= units;
    }
    excerpts
}

/// The first `units` of `text`, cut back to whole lines when that cuts it.
fn head(text: &str, units: usize) -> String {
    let mut count = 0;
    for (at, ch) in text.char_indices() {
        count += ch.len_utf16();
        if count > units {
            return text[..at]
                .rfind('\n')
                .map_or_else(String::new, |line| text[..line].to_string());
        }
    }
    text.to_string()
}

/// The last `units` of `text`, cut back to whole lines when that cuts it.
fn tail(text: &str, units: usize) -> String {
    let mut count = 0;
    let mut start = text.len();
    for (at, ch) in text.char_indices().rev() {
        count += ch.len_utf16();
        if count > units {
            return text[start..]
                .find('\n')
                .map_or_else(String::new, |line| text[start + line + 1..].to_string());
        }
        start = at;
    }
    text.to_string()
}

/// A file's text between `\begin{document}` and `\end{document}`, for a
/// section file that is a document of its own (the subfiles package); the
/// whole text for one that is not.
fn document_body(text: &str) -> &str {
    let blank = blank_comments(text);
    let Some(begin) = blank.find(BEGIN_DOCUMENT) else {
        return text;
    };
    let start = begin + BEGIN_DOCUMENT.len();
    let end = blank[start..]
        .find(END_DOCUMENT)
        .map_or(text.len(), |at| start + at);
    &text[start..end]
}

/// The paths `target`, named in the file at `from`, may be, in the order
/// tried: from the project's root as TeX resolves it, then beside `from`
/// (as the subfiles and import packages do); with `extension` added when it
/// has none.
fn candidates(from: &str, target: &str, extension: &str) -> Vec<String> {
    let target = target.trim();
    if target.is_empty() {
        return Vec::new();
    }
    let named = match target.rsplit('/').next() {
        Some(name) if name.contains('.') => target.to_string(),
        _ => format!("{target}.{extension}"),
    };
    let mut out = vec![normalize(&named)];
    if let Some((folder, _)) = from.rsplit_once('/') {
        let beside = normalize(&format!("{folder}/{named}"));
        if !out.contains(&beside) {
            out.push(beside);
        }
    }
    out.retain(|path| !path.is_empty());
    out
}

/// `path` with its `.` and `..` steps taken, `/`-separated.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// `text` (its comments blanked) with each line's trailing space dropped,
/// and its blank lines too: all of them, or with `paragraphs` all but one
/// between paragraphs.
fn tidy(text: &str, paragraphs: bool) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.split('\n').map(str::trim_end) {
        let blank = line.is_empty();
        if blank && !(paragraphs && lines.last().is_some_and(|last| !last.is_empty())) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// The source with each line's `%` comment blanked (an escaped `\%`
/// stays), one space per byte, so a command found in it is at the same
/// offset in the source.
fn blank_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let mut escaped = false;
        let mut cut = None;
        for (at, c) in line.char_indices() {
            if c == '%' && !escaped {
                cut = Some(at);
                break;
            }
            escaped = c == '\\' && !escaped;
        }
        let Some(at) = cut else {
            out.push_str(line);
            continue;
        };
        out.push_str(&line[..at]);
        for c in line[at..].chars() {
            if c == '\n' {
                out.push('\n');
            } else {
                out.extend(std::iter::repeat_n(' ', c.len_utf8()));
            }
        }
    }
    out
}

/// An include in a file: the bytes `[start, end)` it spans and the files it
/// names.
struct Command {
    start: usize,
    end: usize,
    targets: Vec<String>,
    package: bool,
}

/// The `\input`, `\include` and `\subfile` commands of `blank` (a source
/// with its comments blanked), in order; with `packages`, also
/// `\usepackage` and `\RequirePackage`, which may name several packages.
fn commands(blank: &str, packages: bool) -> Vec<Command> {
    let bytes = blank.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = blank[at..].find('\\') {
        let start = at + found;
        let name_start = start + 1;
        let name_end = blank[name_start..]
            .find(|c: char| !c.is_ascii_alphabetic())
            .map_or(blank.len(), |end| name_start + end);
        let name = &blank[name_start..name_end];
        // Past the name, or past an escaped character such as `\\`.
        at = if name.is_empty() {
            name_start + blank[name_start..].chars().next().map_or(0, char::len_utf8)
        } else {
            name_end
        };
        let package = matches!(name, "usepackage" | "RequirePackage");
        if !(matches!(name, "input" | "include" | "subfile") || packages && package) {
            continue;
        }
        let mut cursor = skip_space(blank, name_end);
        if package && bytes.get(cursor) == Some(&b'[') {
            match blank[cursor..].find(']') {
                Some(close) => cursor = skip_space(blank, cursor + close + 1),
                None => continue,
            }
        }
        if bytes.get(cursor) != Some(&b'{') {
            continue;
        }
        let Some(close) = blank[cursor..].find('}') else {
            continue;
        };
        let argument = &blank[cursor + 1..cursor + close];
        let end = cursor + close + 1;
        at = end;
        let targets = if package {
            argument
                .split(',')
                .map(|name| name.trim().to_string())
                .collect()
        } else {
            vec![argument.trim().to_string()]
        };
        out.push(Command {
            start,
            end,
            targets,
            package,
        });
    }
    out
}

fn skip_space(source: &str, from: usize) -> usize {
    source[from..]
        .find(|c: char| !c.is_whitespace())
        .map_or(source.len(), |at| from + at)
}

#[cfg(test)]
mod tests;
