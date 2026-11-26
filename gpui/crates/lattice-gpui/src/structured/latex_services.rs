//! The Tauri backend commands a `.tex` file's Visual page invokes beside its
//! text (`latex_visual.rs`), answered as the Tauri app answers them, from
//! the same `lattice-core` functions, each off the main thread:
//!
//! - TexLab's completion, hover and definition (`texlab_completion`,
//!   `texlab_hover`, `texlab_definition`, `src/build/texlab-language.ts`), on
//!   the project's TexLab pool, the one the source editor's diagnostics use;
//! - the PDF page of each heading the outline rail lists
//!   (`synctex_heading_pages`);
//! - every citation's uses, for a citation card's count
//!   (`list_citation_usages`).
//!
//! TexLab's requests carry the document's text only when the page thinks
//! TexLab lacks that revision of it, and the page sends the text again only
//! when the refusal it gets is exactly TexLab's `NEEDS_TEXT`. The island
//! bridge hands the page an `Error`, not that bare string, so here the last
//! text the page sent for each path is kept by revision and goes with a
//! request that names that revision: TexLab is never asked without it.
//!
//! The page asks for completions on every letter of a word, with the whole
//! file, which TexLab parses again each time. As in the source editor
//! (`lattice_editor::latex::texlab_completions`), a word TexLab had nothing
//! for is not asked about again for a second while it only grows and the
//! rest of the file stays as it was: the page gets that nothing at once.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use gpui::{Context, Entity};
use lattice_core::{latex::SourceLine, texlab::DocumentAt};
use lattice_editor::{
    latex::texlab_completions::{UNANSWERED_HOLDS, word_start},
    texlab::TexlabService,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::StructuredDocument;

/// What the services need beyond the document: the project's TexLab and the
/// text the page last sent TexLab for each path, by revision.
pub(super) struct LatexServices {
    pub texlab: Entity<TexlabService>,
    texts: SentTexts,
    unanswered: Arc<Mutex<Option<Unanswered>>>,
    /// The last completion asked; only its answer is remembered, as asks
    /// wait for the pool in no particular order.
    latest: Arc<AtomicU64>,
}

impl LatexServices {
    pub fn new(texlab: Entity<TexlabService>) -> Self {
        Self {
            texlab,
            texts: SentTexts::default(),
            unanswered: Arc::default(),
            latest: Arc::default(),
        }
    }
}

/// The text the page last sent TexLab for each path, by revision.
#[derive(Default)]
struct SentTexts(HashMap<String, (u64, String)>);

impl SentTexts {
    /// The text to send TexLab for a request: the request's own, which is
    /// kept for its revision, or the one kept for the revision it names.
    fn text_for(&mut self, request: &TexlabRequest) -> Option<String> {
        match (&request.text, request.revision) {
            (Some(text), Some(revision)) => {
                self.0
                    .insert(request.path.clone(), (revision, text.clone()));
                Some(text.clone())
            }
            (Some(text), None) => Some(text.clone()),
            (None, Some(revision)) => self
                .0
                .get(&request.path)
                .filter(|(kept, _)| *kept == revision)
                .map(|(_, text)| text.clone()),
            (None, None) => None,
        }
    }
}

/// The last word TexLab completed with nothing: the file and its text as
/// they were asked about, and the word, which starts at byte `from`.
struct Unanswered {
    path: String,
    text: String,
    from: usize,
    word: String,
    at: Instant,
}

impl Unanswered {
    /// Whether `word`, starting at `from` in `text`, is this word typed
    /// further with nothing else in the file changed, which TexLab would
    /// have nothing for either.
    fn covers(&self, path: &str, text: &str, from: usize, word: &str) -> bool {
        self.at.elapsed() < UNANSWERED_HOLDS
            && self.path == path
            && self.from == from
            && word.starts_with(&self.word)
            && text.len() - word.len() == self.text.len() - self.word.len()
            && text[..from] == self.text[..from]
            && text[from + word.len()..] == self.text[from + self.word.len()..]
    }
}

/// The word before the 1-based `line` and UTF-16 `character` the page
/// asks at, and the byte where it starts in `text`, when there is one.
fn word_at(text: &str, line: u32, character: u32) -> Option<(usize, &str)> {
    let line_start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1) as usize)
        .map(str::len)
        .sum();
    let mut units = character.saturating_sub(1) as usize;
    let mut caret = line_start;
    for character in text[line_start..].chars() {
        if units == 0 || character == '\n' {
            break;
        }
        units = units.saturating_sub(character.len_utf16());
        caret += character.len_utf8();
    }
    let from = line_start + word_start(&text[line_start..caret])?;
    Some((from, &text[from..caret]))
}

/// A TexLab request as `texlab-language.ts` sends it.
#[derive(Deserialize)]
struct TexlabRequest {
    path: String,
    text: Option<String>,
    revision: Option<u64>,
    line: u32,
    character: u32,
}

#[derive(Deserialize)]
struct HeadingsRequest {
    headings: Vec<SourceLine>,
}

fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|error| error.to_string())
}

fn json<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

impl StructuredDocument {
    /// One of the backend commands above; false when `command` is not one.
    pub(super) fn invoke_latex_service(
        &mut self,
        id: u64,
        command: &str,
        args: &Value,
        cx: &mut Context<Self>,
    ) -> bool {
        match command {
            "texlab_completion" | "texlab_hover" | "texlab_definition" => {
                self.ask_texlab(id, command, args, cx)
            }
            "synctex_heading_pages" => {
                let request = parse::<HeadingsRequest>(args);
                let root = self.root.clone();
                self.answer_off_thread(id, cx, move || {
                    json(lattice_core::latex::heading_pages(
                        &root,
                        &request?.headings,
                    )?)
                });
            }
            "list_citation_usages" => {
                let root = self.root.clone();
                self.answer_off_thread(id, cx, move || {
                    json(lattice_core::project::citation_usages(&root)?)
                });
            }
            _ => return false,
        }
        true
    }

    /// A TexLab request at a position, on the project's pool.
    fn ask_texlab(&mut self, id: u64, command: &str, args: &Value, cx: &mut Context<Self>) {
        let request = match parse::<TexlabRequest>(args) {
            Ok(request) => request,
            Err(error) => return self.respond(id, Err(error), cx),
        };
        let Some(services) = &mut self.latex_services else {
            return self.respond(id, Err("TexLab is unavailable.".into()), cx);
        };
        let text = services.texts.text_for(&request);
        let word = text
            .as_deref()
            .filter(|_| command == "texlab_completion")
            .and_then(|text| word_at(text, request.line, request.character))
            .map(|(from, word)| (from, word.to_string()));
        let unanswered = services.unanswered.clone();
        if let (Some(text), Some((from, word))) = (&text, &word)
            && unanswered.lock().is_ok_and(|unanswered| {
                unanswered
                    .as_ref()
                    .is_some_and(|known| known.covers(&request.path, text, *from, word))
            })
        {
            return self.respond(id, Ok(json!([])), cx);
        }
        let latest = services.latest.clone();
        let ask = latest.fetch_add(1, Ordering::SeqCst) + 1;
        let (root, pool) = services.texlab.read(cx).pool();
        let command = command.to_string();
        self.answer_off_thread(id, cx, move || {
            let mut pool = pool
                .lock()
                .map_err(|_| "TexLab state is unavailable.".to_string())?;
            let at = DocumentAt {
                path: &request.path,
                text: text.as_deref(),
                revision: request.revision,
                line: request.line,
                character: request.character,
            };
            match command.as_str() {
                "texlab_completion" => {
                    let items = pool.completion(&root, &at)?;
                    drop(pool);
                    if let Ok(mut unanswered) = unanswered.lock()
                        && latest.load(Ordering::SeqCst) == ask
                    {
                        *unanswered = match (items.is_empty(), text, word) {
                            (true, Some(text), Some((from, word))) => Some(Unanswered {
                                path: request.path,
                                text,
                                from,
                                word,
                                at: Instant::now(),
                            }),
                            _ => None,
                        };
                    }
                    json(items)
                }
                "texlab_hover" => json(pool.hover(&root, &at)?),
                _ => json(pool.definition(&root, &at)?),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &str, text: Option<&str>, revision: Option<u64>) -> TexlabRequest {
        TexlabRequest {
            path: path.into(),
            text: text.map(str::to_string),
            revision,
            line: 1,
            character: 1,
        }
    }

    /// `texlab-language.ts` sends a revision's text once, then asks without
    /// it: the text it sent is what goes with those requests, and a newer
    /// revision it has not sent goes without, as TexLab would refuse it.
    #[test]
    fn a_request_without_text_takes_the_text_sent_for_its_revision() {
        let mut texts = SentTexts::default();
        assert_eq!(
            texts
                .text_for(&request("main.tex", Some("v1"), Some(1)))
                .as_deref(),
            Some("v1")
        );
        assert_eq!(
            texts
                .text_for(&request("main.tex", None, Some(1)))
                .as_deref(),
            Some("v1")
        );
        assert_eq!(texts.text_for(&request("main.tex", None, Some(2))), None);
        assert_eq!(texts.text_for(&request("other.tex", None, Some(1))), None);
    }

    /// The page's position is a 1-based line and UTF-16 column; the word is
    /// a command or letters before it.
    #[test]
    fn the_word_before_the_pages_position() {
        let text = "\\section{Intro}\nSome prose héllo wor\nnext";
        assert_eq!(word_at(text, 2, 21), Some((34, "wor")));
        assert_eq!(word_at(text, 1, 9), Some((0, "\\section")));
        assert_eq!(word_at(text, 2, 5), Some((16, "Some")));
        assert_eq!(word_at(text, 2, 6), None);
        assert_eq!(word_at(text, 2, 1), None);
        assert_eq!(word_at(text, 9, 1), None);
        // An emoji is two UTF-16 units and four bytes.
        assert_eq!(word_at("\u{1F600} ab", 1, 6), Some((5, "ab")));
    }

    /// A word TexLab had nothing for keeps that nothing while only letters
    /// are added to it, not once anything else in the file changes.
    #[test]
    fn only_letters_added_to_the_word_keep_texlabs_nothing() {
        let known = Unanswered {
            path: "main.tex".into(),
            text: "a wor b".into(),
            from: 2,
            word: "wor".into(),
            at: Instant::now(),
        };
        assert!(known.covers("main.tex", "a word b", 2, "word"));
        assert!(!known.covers("other.tex", "a word b", 2, "word"));
        assert!(!known.covers("main.tex", "x word b", 2, "word"));
        assert!(!known.covers("main.tex", "a word c", 2, "word"));
        assert!(!known.covers("main.tex", "a wo b", 2, "wo"));
        assert!(!known.covers("main.tex", "a  word b", 3, "word"));
        assert!(!known.covers("main.tex", "a word bb", 2, "word"));
    }
}
