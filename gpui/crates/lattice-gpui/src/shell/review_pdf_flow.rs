//! The window's side of Export review PDF (`crate::pdf::review_sheet`):
//! opened from a PDF's overflow menu over that PDF, and from the
//! Annotations panel over the live PDF, with the project's local notes as
//! they are then and the writer's name for the cover note
//! (`annotationRounds.openReviewPdf`).

use gpui::{AnyElement, AppContext as _, Context, Entity, IntoElement as _, Subscription, Window};

use super::{
    Shell,
    parts::{Part, no_commands},
};
use crate::{
    annotations::store,
    pdf::{
        PdfView,
        review_sheet::{ReviewSheet, ReviewSheetEvent},
    },
};

pub(super) const PART: Part = Part {
    install: |_, _| Box::new(ReviewFlow::default()),
    views: &[],
    actions: |element, _| element,
    commands: no_commands,
};

#[derive(Default)]
pub(crate) struct ReviewFlow {
    sheet: Option<(Entity<ReviewSheet>, Subscription)>,
}

impl Shell {
    /// Show Export review PDF over `view`'s PDF (`None`: the live one).
    pub(crate) fn open_review_sheet(
        &mut self,
        view: Option<Entity<PdfView>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.root(cx) else {
            return;
        };
        let view = view.unwrap_or_else(|| self.pdf.read(cx).live_view().clone());
        let notes: Vec<_> = store::try_store(cx)
            .map(|store| {
                store
                    .read(cx)
                    .annotations(&root)
                    .into_iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let author = store::author(&root, cx).name;
        let sheet = cx.new(|cx| ReviewSheet::new(&view, notes, author, cx));
        let subscription =
            cx.subscribe_in(
                &sheet,
                window,
                move |this, _, event, window, cx| match event {
                    ReviewSheetEvent::Close => {
                        this.parts.get_mut::<ReviewFlow>().sheet = None;
                        cx.notify();
                    }
                    // A build compiles the root of the file in front, so the root
                    // asked for opens first; the sheet stays up over it.
                    ReviewSheetEvent::Build(document) => {
                        if this
                            .open_path(root.join(document), None, window, cx)
                            .is_some()
                        {
                            this.build(&super::Build, window, cx);
                        }
                    }
                },
            );
        self.parts.get_mut::<ReviewFlow>().sheet = Some((sheet, subscription));
        cx.notify();
    }

    /// A build finished: the sheet waiting on a root's build hears whether
    /// this was it, and the digest of the PDF it wrote.
    pub(super) fn review_build_finished(
        &mut self,
        outcome: Option<&Result<super::build::Finished, String>>,
        cx: &mut Context<Self>,
    ) {
        let Some((sheet, _)) = &self.parts.get::<ReviewFlow>().sheet else {
            return;
        };
        let sheet = sheet.clone();
        let Some(wanted) = sheet.read(cx).building().map(str::to_string) else {
            return;
        };
        let finished = outcome.and_then(|outcome| outcome.as_ref().ok());
        let other = finished.is_some_and(|finished| finished.result.root_document != wanted);
        // Another root's build ran first; the one asked for is queued.
        if other && self.queued_build.is_some() {
            return;
        }
        let digest = finished
            .filter(|finished| finished.result.success && !other)
            .and_then(|finished| finished.pdf_digest.clone());
        sheet.update(cx, |sheet, cx| sheet.build_finished(digest, cx));
    }

    /// The sheet, for the window to draw over everything.
    pub(super) fn review_sheet(&self) -> Option<AnyElement> {
        let (sheet, _) = self.parts.get::<ReviewFlow>().sheet.as_ref()?;
        Some(sheet.clone().into_any_element())
    }

    /// The sheet, while it is up (for a scenario).
    pub(crate) fn review_sheet_entity(&self) -> Option<Entity<ReviewSheet>> {
        self.parts
            .get::<ReviewFlow>()
            .sheet
            .as_ref()
            .map(|(sheet, _)| sheet.clone())
    }
}
