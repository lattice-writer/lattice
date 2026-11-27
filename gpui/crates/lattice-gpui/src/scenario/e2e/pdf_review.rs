//! Export review PDF (`review-pdf-sheet.tsx`): with a note on words of
//! chapters/intro.tex, the sheet over the live PDF finds the words and
//! offers the note ticked as a highlight; exported, the copy carries the
//! note as a Highlight annotation whose `/NM` is the note's id and whose
//! comment starts with its pin number, and the cover note on page 1, while
//! the compiled PDF itself is left as it was.

use std::{path::Path, time::Duration};

use anyhow::Result;
use gpui::{AsyncApp, Entity};
use lopdf::{Document, Object};
use serde_json::json;

use super::Steps;
use crate::{
    annotations::{model, store},
    pdf::review_sheet::ReviewSheet,
    scenario::Run,
};

/// Words of the fixture's introduction that earlier modules leave as they are.
const WORDS: &str = "paragraph introduces the document";

/// Each annotation of the PDF at `path`: its subtype, `/NM` and contents.
fn annotations_of(path: &Path) -> Vec<(String, String, String)> {
    let Ok(document) = Document::load(path) else {
        return Vec::new();
    };
    let text = |object: Option<&Object>| match object {
        Some(Object::String(bytes, _)) => {
            if bytes.starts_with(&[0xfe, 0xff]) {
                let units: Vec<u16> = bytes[2..]
                    .chunks(2)
                    .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
                    .collect();
                String::from_utf16_lossy(&units)
            } else {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
        Some(Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
        _ => String::new(),
    };
    let mut found = Vec::new();
    for (_, page) in document.get_pages() {
        let Ok(annots) = document
            .get_dictionary(page)
            .and_then(|page| page.get(b"Annots"))
            .and_then(|annots| {
                document
                    .dereference(annots)
                    .map(|(_, object)| object.clone())
            })
            .and_then(|object| object.as_array().cloned())
        else {
            continue;
        };
        for annot in annots {
            let Ok((_, Object::Dictionary(annot))) = document.dereference(&annot) else {
                continue;
            };
            found.push((
                text(annot.get(b"Subtype").ok()),
                text(annot.get(b"NM").ok()),
                text(annot.get(b"Contents").ok()),
            ));
        }
    }
    found
}

impl Run {
    async fn pdf_review(&self, cx: &mut AsyncApp, steps: &mut Steps, root: &Path) -> Result<()> {
        const STEP: &str = "PDF: Export review PDF places a note on its words as a highlight, ticked, and the copy carries it as a Highlight with its id and number, with the cover note on page 1, leaving the compiled PDF as it was";
        let path = "chapters/intro.tex";
        let text = std::fs::read_to_string(root.join(path))?;
        let (Some(start), Some(project)) = (
            text.find(WORDS),
            cx.update(|cx| self.shell.read(cx).root(cx)),
        ) else {
            steps.check(STEP, false, json!({ "fixture": false }));
            return Ok(());
        };
        // The note's words are typeset in main.tex's PDF, so it must be live.
        // An earlier module's document left open can take it: a build that
        // runs while that editor is the active one typesets it, the file
        // open winning as Overleaf's rule has it (`open_document`), and on
        // its PDF the note is rightly unplaced.
        self.open(cx, &root.join("main.tex"), Some(1))?;
        self.pause(cx, Duration::from_millis(300)).await;
        self.build(cx).await?;
        self.wait_for(cx, Duration::from_secs(20), |shell, cx| {
            shell.read(cx).live_document() == Some("main.tex")
        })
        .await;
        let live = cx.update(|cx| self.shell.read(cx).live_document().map(str::to_string));
        let store = cx.update(store::store);
        cx.update(|cx| store.update(cx, |store, cx| store.ensure(&project, cx)));
        self.wait_for(cx, Duration::from_secs(10), |_, cx| {
            store.read(cx).loaded(&project)
        })
        .await;
        let id = cx
            .update(|cx| {
                let author = store::author(&project, cx);
                let note = model::new_note(
                    path,
                    &text,
                    start,
                    start + WORDS.len(),
                    "Name what it introduces.",
                    None,
                    &author,
                )?;
                store.update(cx, |store, cx| store.add(&project, note, cx))
            })
            .unwrap_or_default();
        let number = cx.update(|cx| {
            store
                .read(cx)
                .get(&project, &id)
                .map_or(0, |note| note.number)
        });
        // The live PDF learns of the note before the sheet reads it.
        let view = cx.update(|cx| self.shell.read(cx).pdf().read(cx).live_view().clone());
        self.wait_for(cx, Duration::from_secs(10), |_, cx| {
            view.read(cx)
                .note_places()
                .iter()
                .any(|(placed, _)| *placed == id)
        })
        .await;
        let compiled = cx.update(|cx| view.read(cx).review_source().map(|source| source.path));
        let before = compiled.as_ref().and_then(|path| std::fs::read(path).ok());
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.open_review_sheet(None, window, cx))
        })?;
        let sheet = cx.update(|cx| self.shell.read(cx).review_sheet_entity());
        let placed = match &sheet {
            Some(sheet) => {
                self.wait_for(cx, Duration::from_secs(20), |_, cx| {
                    sheet.read(cx).placed().is_some()
                })
                .await
            }
            None => false,
        };
        let (kinds, ticked) = sheet.as_ref().map_or((None, Vec::new()), |sheet| {
            cx.update(|cx| {
                let sheet = sheet.read(cx);
                (
                    sheet.placed().map(|placed| {
                        placed
                            .into_iter()
                            .map(|(id, kind)| (id, kind.to_string()))
                            .collect::<Vec<_>>()
                    }),
                    sheet.ticked(),
                )
            })
        });
        self.snapshot(cx, "pdf-review-sheet").await;
        let highlighted = kinds.as_ref().is_some_and(|kinds| {
            kinds
                .iter()
                .any(|(placed, kind)| *placed == id && kind == "highlight")
        });
        let destination = root.with_file_name("review-copy.pdf");
        let _ = std::fs::remove_file(&destination);
        if let Some(sheet) = &sheet {
            let destination = destination.clone();
            self.with_window(cx, {
                let sheet = sheet.clone();
                move |_, window, cx| {
                    sheet.update(cx, |sheet, cx| sheet.export_to(destination, window, cx))
                }
            })?;
        }
        let closed = self
            .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                shell.read(cx).review_sheet_entity().is_none()
            })
            .await;
        let failure = sheet
            .as_ref()
            .and_then(|sheet| cx.update(|cx| sheet.read(cx).failure().map(str::to_string)));
        let annotations = annotations_of(&destination);
        let carried = annotations.iter().any(|(kind, nm, contents)| {
            // lattice-core names each `lattice-annotation-<id>`.
            kind == "Highlight" && nm.ends_with(&id) && contents.starts_with(&format!("#{number}"))
        });
        let cover = annotations
            .iter()
            .any(|(kind, nm, _)| kind == "Text" && nm.ends_with("review-cover"));
        let untouched = before.is_some()
            && compiled.as_ref().and_then(|path| std::fs::read(path).ok()) == before;
        steps.check(
            STEP,
            live.as_deref() == Some("main.tex")
                && placed
                && highlighted
                && ticked.contains(&id)
                && closed
                && carried
                && cover
                && untouched,
            json!({
                "placed": placed, "kinds": kinds, "ticked": ticked, "closed": closed,
                "failure": failure, "annotations": annotations, "untouched": untouched, "live": live,
            }),
        );
        // An export that never ran (nothing placed to tick) leaves the sheet
        // up, over every module after this one.
        if let Some(sheet) = &sheet
            && !closed
        {
            cx.update(|cx| sheet.update(cx, |sheet, cx| sheet.close_for_scenario(cx)));
        }
        cx.update(|cx| store.update(cx, |store, cx| store.delete(&project, &id, cx)));
        self.pdf_review_elsewhere(cx, steps, root, &text, start).await
    }

    /// With another root's PDF live, the sheet says which root typesets the
    /// note and builds it on request, after which the note is placed.
    async fn pdf_review_elsewhere(
        &self,
        cx: &mut AsyncApp,
        steps: &mut Steps,
        root: &Path,
        text: &str,
        start: usize,
    ) -> Result<()> {
        const STEP: &str = "PDF: on another document's PDF, Export review PDF names the document a note is in, and its Build places the note there";
        let other = root.join("review-other.tex");
        // The build adds the document to the manifest's roots; put it back.
        let manifest = root.join(".research/project.json");
        let manifest_before = std::fs::read(&manifest)?;
        std::fs::write(
            &other,
            "\\documentclass{article}\n\\begin{document}\nAnother document.\n\\end{document}\n",
        )?;
        let editor = self.open(cx, &other, Some(3))?;
        self.pause(cx, Duration::from_millis(300)).await;
        self.build(cx).await?;
        let other_live = self
            .wait_for(cx, Duration::from_secs(20), |shell, cx| {
                shell.read(cx).live_document() == Some("review-other.tex")
            })
            .await;
        let Some(project) = cx.update(|cx| self.shell.read(cx).root(cx)) else {
            steps.check(STEP, false, json!({ "project": false }));
            return Ok(());
        };
        let store = cx.update(store::store);
        let id = cx
            .update(|cx| {
                let author = store::author(&project, cx);
                let note = model::new_note(
                    "chapters/intro.tex",
                    text,
                    start,
                    start + WORDS.len(),
                    "Name what it introduces.",
                    None,
                    &author,
                )?;
                store.update(cx, |store, cx| store.add(&project, note, cx))
            })
            .unwrap_or_default();
        // The sheet takes the live PDF's review source once, when it opens,
        // and places nothing without one. live_document() names
        // review-other.tex before the view has loaded review-other.pdf, and a
        // sheet opened during that load never places: v0.20.2's GPUI tag run
        // showed it on macOS and Linux (before: null, elsewhere: []). The
        // Annotations panel's button waits for the same source (can_review).
        let view = cx.update(|cx| self.shell.read(cx).pdf().read(cx).live_view().clone());
        let source = self
            .wait_for(cx, Duration::from_secs(20), |_, cx| {
                view.read(cx)
                    .review_source()
                    .is_some_and(|source| source.shows("review-other.tex"))
            })
            .await;
        self.with_window(cx, |shell, window, cx| {
            shell.update(cx, |shell, cx| shell.open_review_sheet(None, window, cx))
        })?;
        let sheet = cx.update(|cx| self.shell.read(cx).review_sheet_entity());
        let kind = |sheet: &Entity<ReviewSheet>, cx: &mut AsyncApp| {
            cx.update(|cx| {
                sheet.read(cx).placed().and_then(|placed| {
                    placed
                        .into_iter()
                        .find(|(placed, _)| *placed == id)
                        .map(|(_, kind)| kind)
                })
            })
        };
        let (mut before, mut elsewhere, mut after) = (None, Vec::new(), None);
        if let Some(sheet) = &sheet {
            self.wait_for(cx, Duration::from_secs(20), |_, cx| {
                sheet.read(cx).placed().is_some()
            })
            .await;
            before = kind(sheet, cx);
            elsewhere = cx.update(|cx| sheet.read(cx).elsewhere().to_vec());
            self.snapshot(cx, "pdf-review-elsewhere").await;
            if elsewhere.iter().any(|(document, _)| document == "main.tex") {
                cx.update(|cx| {
                    sheet.update(cx, |sheet, cx| sheet.build_for_scenario("main.tex", cx))
                });
                self.wait_for(cx, Duration::from_secs(60), |_, cx| {
                    sheet.read(cx).placed().is_some_and(|placed| {
                        placed
                            .iter()
                            .any(|(placed, kind)| *placed == id && *kind == "highlight")
                    })
                })
                .await;
                after = kind(sheet, cx);
            }
            cx.update(|cx| sheet.update(cx, |sheet, cx| sheet.close_for_scenario(cx)));
        }
        let live = cx.update(|cx| self.shell.read(cx).live_document().map(str::to_string));
        steps.check(
            STEP,
            other_live
                && before == Some("unplaced")
                && elsewhere == vec![("main.tex".to_string(), 1)]
                && after == Some("highlight")
                && live.as_deref() == Some("main.tex"),
            json!({
                "other_live": other_live, "source": source, "before": before,
                "elsewhere": elsewhere, "after": after, "live": live,
            }),
        );
        // Leave no document of this step to take the live PDF later, as an
        // open root whose editor is in front would (LEO-134).
        cx.update(|cx| store.update(cx, |store, cx| store.delete(&project, &id, cx)));
        self.close_panel(cx, &editor).await?;
        cx.update(|cx| {
            let workspace = self.shell.read(cx).workspace().clone();
            workspace.update(cx, |workspace, cx| {
                workspace.close(&[crate::shell::views::root_pdf_id("review-other.tex")], cx)
            });
        });
        for extension in ["tex", "pdf", "aux", "log", "synctex.gz", "fls", "fdb_latexmk"] {
            std::fs::remove_file(other.with_extension(extension)).ok();
        }
        std::fs::write(&manifest, &manifest_before)?;
        cx.update(|cx| {
            if let Some(project) = self.shell.read(cx).project().cloned() {
                project.update(cx, |project, cx| project.reload_manifest(cx));
            }
        });
        self.pause(cx, Duration::from_millis(300)).await;
        Ok(())
    }
}

/// This file's registration as the e2e module `pdf_review` (`registry.rs`).
pub(super) const MODULE: super::registry::Module = super::registry::Module {
    after: super::registry::After::Repair,
    rank: 53,
    needs: &[],
    setup: false,
    covers: &[
        "gpui/crates/lattice-gpui/src/pdf/review_place.rs",
        "gpui/crates/lattice-gpui/src/pdf/review_sheet.rs",
        "gpui/crates/lattice-gpui/src/pdf/view/review.rs",
        "gpui/crates/lattice-gpui/src/shell/review_pdf_flow.rs",
    ],
    run: |run, cx, steps, fixture| Box::pin(run.pdf_review(cx, steps, &fixture.root)),
};
