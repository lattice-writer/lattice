//! A tab following its file when it is renamed or moved within the project,
//! by the tree or outside Lattice: the path, the LaTeX and spelling
//! features that read it, its language, folds and outline rail.

use std::time::Duration;

use futures::FutureExt as _;

use super::*;

impl EditorPanel {
    /// The file was renamed or moved within the project (by the tree, or
    /// outside Lattice): the tab, its buffer and its history follow it.
    pub fn moved_to(&mut self, relative: String, cx: &mut Context<Self>) {
        let root = self.project.read(cx).root().to_path_buf();
        self.path = root.join(&relative);
        if let Some(diagnostics) = &self.diagnostics {
            let relative = relative.clone();
            diagnostics.update(cx, |diagnostics, cx| diagnostics.set_path(relative, cx));
        }
        if let Some(document) = self
            .structured
            .clone()
            .or_else(|| self.markdown_document().cloned())
        {
            let relative = relative.clone();
            document.update(cx, |document, cx| document.moved_to(relative, cx));
        }
        if let Some(document) = self.latex_visual_document().cloned() {
            let relative = relative.clone();
            document.update(cx, |document, cx| document.moved_to(relative, cx));
        }
        let left = std::mem::replace(&mut self.relative, relative);
        self.place_moved(&left, cx);
        match &self.index_lint {
            Some(lint) => {
                let relative = self.relative.clone();
                lint.update(cx, |lint, cx| lint.set_path(relative, cx));
            }
            None => {
                if let Some(facts) = self.attach_tex(cx) {
                    self.attach_index_lint(facts, cx);
                }
            }
        }
        crate::status_counters::set_buffer(root.clone(), left, None, cx);
        self.scan_todos(Duration::ZERO, cx);
        self.bib_moved(cx);
        // The citation cards' library knows the file by its new path.
        if let Some(papers) = self.bib.papers.clone() {
            self.attach_citations(&papers, cx);
        }
        self.spelling_moved(cx);
        self.disk = DiskVersion::Unread;
        match latex::language_for_path(&self.path, cx) {
            Some(language) => self
                .buffer
                .update(cx, |buffer, cx| buffer.set_language(Some(language), cx)),
            None => self.set_code_language(cx),
        }
        let tex = latex::is_tex_source(&self.path);
        self.lattice.update(cx, |lattice, cx| {
            lattice.set_fold_source(tex.then(latex::folding::source), cx)
        });
        match (&self.rail, tex) {
            (Some(rail), true) => {
                let path = self.relative.clone();
                let source = self.buffer.read(cx).text();
                rail.update(cx, |rail, cx| rail.set_source(path, source, cx));
            }
            (Some(_), false) => self.rail = None,
            (None, true) => {
                let source = self.buffer.read(cx).text();
                self.rail = Some(Self::new_rail(
                    &self.lattice,
                    &self.project,
                    self.relative.clone(),
                    source,
                    cx,
                ));
            }
            (None, false) => {}
        }
        self.offer_view_switch(cx);
        crate::crash::note_editor(self, cx);
        cx.notify();
    }

    /// Give a file `latex::language_for_path` has no language for its code
    /// language (Python, Rust, a Makefile…), or none: at once when the
    /// registry has built it, so a renamed file keeps its colours, else once
    /// it is built, the first such file of a session waiting about a frame
    /// for its query (`lattice_editor::code_languages`). A file renamed
    /// meanwhile keeps what its new name chose.
    pub(super) fn set_code_language(&self, cx: &mut Context<Self>) {
        let mut load = lattice_editor::code_languages::load_for_path(&self.path, cx).map(Box::pin);
        let ready = match load.as_mut() {
            Some(load) => load.as_mut().now_or_never(),
            None => Some(None),
        };
        if let Some(language) = ready {
            self.buffer
                .update(cx, |buffer, cx| buffer.set_language(language, cx));
            return;
        }
        if self.buffer.read(cx).language().is_some() {
            self.buffer
                .update(cx, |buffer, cx| buffer.set_language(None, cx));
        }
        let Some(load) = load else { return };
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let language = load.await?;
            this.update(cx, |this, cx| {
                if this.path == path {
                    this.buffer
                        .update(cx, |buffer, cx| buffer.set_language(Some(language), cx));
                }
            })
            .ok()
        })
        .detach();
    }
}
