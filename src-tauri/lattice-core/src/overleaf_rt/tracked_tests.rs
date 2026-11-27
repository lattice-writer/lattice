//! Suggestions riding along with the text: a suggested deletion is a point
//! (its text has left the document), a suggested insertion is a span, and an
//! edit sent as a suggestion adds to them the way Overleaf's review panel
//! shows it. Ported case for case from the Tauri app's
//! `src/overleaf/overleaf-suggestions.test.ts`.

use super::*;
use crate::overleaf_rt::ot::{apply_ops, delete, insert};

fn ada() -> Suggester {
    Suggester { user_id: Some("ada".into()), hue: 30 }
}

fn grace() -> Suggester {
    Suggester { user_id: Some("grace".into()), hue: 250 }
}

fn change(position: i64, text: &str, by: &Suggester, id: &str, deletion: bool) -> TrackedChange {
    TrackedChange {
        id: id.into(),
        position,
        text: text.into(),
        deletion,
        user_id: by.user_id.clone(),
        timestamp: None,
        hue: by.hue,
    }
}

fn insertion(position: i64, text: &str) -> TrackedChange {
    change(position, text, &grace(), "ins", false)
}

fn deletion(position: i64, text: &str) -> TrackedChange {
    change(position, text, &grace(), "del", true)
}

/// What review shows for `changes`: each one's kind, where it sits and its
/// text, sorted as the TS test sorts them.
fn shown(changes: &[TrackedChange]) -> Vec<String> {
    let mut shown: Vec<String> = changes
        .iter()
        .map(|change| {
            let kind = if change.deletion { "-" } else { "+" };
            format!("{kind}{}:{}", change.position, change.text)
        })
        .collect();
    shown.sort();
    shown
}

/// Apply `ops` to `text` as suggestions from `by` (or as ordinary edits),
/// checking each suggested insertion still spans its text.
fn edit(
    text: &str, changes: &[TrackedChange], ops: &[OtOp], by: Option<&Suggester>,
) -> (String, Vec<TrackedChange>) {
    let after = apply_ops(text, ops).expect("ops fit the text");
    let next = changes_after(changes, ops, by, "2026-10-22T00:00:00.000Z");
    for change in next.iter().filter(|change| !change.deletion) {
        let from = change.position;
        let to = from + change.text.encode_utf16().count() as i64;
        assert_eq!(slice(&after, from, to), change.text, "{change:?} spans its text");
    }
    (after, next)
}

#[test]
fn ordinary_edits_keep_a_suggested_deletion_as_a_point() {
    // "The brown fox", with "quick " suggested away at 4: text after it moves
    // nothing, text before moves it.
    let removed = [deletion(4, "quick ")];
    let at = |op| shown(&edit("The brown fox", &removed, &[op], None).1);
    assert_eq!(at(delete(4, "brown ")), ["-4:quick "]);
    assert_eq!(at(insert(10, "red ")), ["-4:quick "]);
    assert_eq!(at(insert(0, "See: ")), ["-9:quick "]);
    assert_eq!(at(delete(0, "The ")), ["-0:quick "]);
}

#[test]
fn ordinary_edits_never_drop_a_deletion_nor_type_inside_it() {
    let removed = [deletion(4, "quick ")];
    // Used to read as a five-character span that "lost" its text and
    // vanished here while the suggestion still stood on Overleaf.
    let (_, changes) = edit("The brown fox", &removed, &[delete(2, "e brown")], None);
    assert_eq!(shown(&changes), ["-2:quick "]);
    // Typed at the point it follows the removed text, as a replacement reads.
    let (_, changes) = edit("The brown fox", &removed, &[insert(4, "fast ")], None);
    assert_eq!(shown(&changes), ["-4:quick "]);
}

#[test]
fn ordinary_edits_keep_an_insertion_in_step_and_drop_it_once_gone() {
    let added = [insertion(4, "quick ")];
    let at = |op| shown(&edit("The quick brown fox", &added, &[op], None).1);
    assert_eq!(at(delete(6, "ic")), ["+4:quk "]);
    assert_eq!(at(delete(2, "e qu")), ["+2:ick "]);
    assert_eq!(at(insert(6, "XX")), ["+4:quXXick "]);
    // Typing at either edge stays outside it.
    assert_eq!(at(insert(4, "a ")), ["+6:quick "]);
    assert_eq!(at(insert(10, "a ")), ["+4:quick "]);
    let (_, changes) = edit("The quick brown fox", &added, &[delete(3, " quick b")], None);
    assert!(changes.is_empty());
}

#[test]
fn ordinary_edits_move_comments_and_leave_untouched_changes_as_they_were() {
    let untouched = insertion(10, "brown");
    let comments = [CommentRange { thread_id: "t".into(), position: 16, quote: "fox".into() }];
    let ops = [insert(19, "!")];
    assert_eq!(comments_after(&comments, &ops)[0].position, 16);
    let changes = changes_after(std::slice::from_ref(&untouched), &ops, None, "now");
    assert_eq!(changes[0], untouched);
    // An insert before the span moves it; a delete across its start pulls it
    // back to where the delete began.
    assert_eq!(comments_after(&comments, &[insert(0, "ab")])[0].position, 18);
    assert_eq!(comments_after(&comments, &[delete(14, "xxx")])[0].position, 14);
}

/// A comment on `quote` at `position` of `text`, carried across `ops`:
/// the text they leave, and the comment's position and quote. While the
/// comment is attached, its quote must be exactly the text it covers, which
/// is what Overleaf's ranges tracker checks (`validate`).
fn comment_through(text: &str, position: i64, quote: &str, ops: &[OtOp]) -> (String, i64, String) {
    let comments = [CommentRange { thread_id: "t".into(), position, quote: quote.into() }];
    let text = apply_ops(text, ops).expect("ops fit the text");
    let after = comments_after(&comments, ops).remove(0);
    let covered = slice(&text, after.position, after.position + len(&after.quote));
    assert_eq!(covered, after.quote, "the quote is the text it covers");
    (text, after.position, after.quote)
}

#[test]
fn cutting_or_deleting_a_whole_commented_passage_detaches_its_comment() {
    // #1399: "quick " commented, followed by "brown fox". Cut and Delete both
    // reach Overleaf as one deletion of the passage.
    let (text, position, quote) =
        comment_through("The quick brown fox", 4, "quick ", &[delete(4, "quick ")]);
    assert_eq!(text, "The brown fox");
    // Detached where the passage was, covering nothing: never "brown ".
    assert_eq!((position, quote.as_str()), (4, ""));
    // A deletion reaching past both edges detaches it too.
    let (_, position, quote) =
        comment_through("The quick brown fox", 4, "quick", &[delete(3, " quick ")]);
    assert_eq!((position, quote.as_str()), (3, ""));
}

#[test]
fn partly_deleting_a_commented_passage_leaves_the_rest_commented() {
    let at = |ops: &[OtOp]| {
        let (_, position, quote) = comment_through("The quick brown fox", 4, "quick brown", ops);
        (position, quote)
    };
    assert_eq!(at(&[delete(4, "quick ")]), (4, "brown".into()));
    assert_eq!(at(&[delete(9, " brown")]), (4, "quick".into()));
    assert_eq!(at(&[delete(6, "ick br")]), (4, "quown".into()));
    // Across its start: what is left of it begins where the deletion did.
    assert_eq!(at(&[delete(0, "The qu")]), (0, "ick brown".into()));
    // Across its end.
    assert_eq!(at(&[delete(10, "brown fox")]), (4, "quick ".into()));
}

#[test]
fn text_typed_inside_a_comment_joins_it_and_at_its_edges_stays_out() {
    let at = |ops: &[OtOp]| {
        let (_, position, quote) = comment_through("The quick fox", 4, "quick", ops);
        (position, quote)
    };
    assert_eq!(at(&[insert(6, "XX")]), (4, "quXXick".into()));
    assert_eq!(at(&[insert(4, "a ")]), (6, "quick".into()));
    assert_eq!(at(&[insert(9, "est")]), (4, "quick".into()));
}

#[test]
fn undo_or_paste_back_does_not_reattach_a_detached_comment_by_itself() {
    // Overleaf's ranges tracker moves a detached comment past text typed at
    // its point, so the passage coming back (Undo, or pasting the cut text
    // where it was) leaves the comment detached; only a comment op from the
    // editor re-attaches it. The mark must not reappear on the restored
    // text, nor on the text after it.
    let ops = [delete(4, "quick "), insert(4, "quick ")];
    let (text, position, quote) = comment_through("The quick brown fox", 4, "quick ", &ops);
    assert_eq!(text, "The quick brown fox");
    assert_eq!((position, quote.as_str()), (10, ""));
    // Pasted somewhere else: still detached, moved along by nothing.
    let ops = [delete(4, "quick "), insert(13, " quick")];
    let (text, position, quote) = comment_through("The quick brown fox", 4, "quick ", &ops);
    assert_eq!(text, "The brown fox quick");
    assert_eq!((position, quote.as_str()), (4, ""));
}

#[test]
fn a_detached_comment_moves_only_with_deletions_before_or_across_its_point() {
    // A deletion spanning the point pulls it back to where it began.
    let ops = [delete(4, "quick "), delete(2, "e brown")];
    let (_, position, quote) = comment_through("The quick brown fox", 4, "quick ", &ops);
    assert_eq!((position, quote.as_str()), (2, ""));
    // One starting at the point leaves it there.
    let ops = [delete(4, "quick "), delete(4, "brown ")];
    let (_, position, quote) = comment_through("The quick brown fox", 4, "quick ", &ops);
    assert_eq!((position, quote.as_str()), (4, ""));
}

#[test]
fn each_op_edits_the_quote_the_last_one_left() {
    // The deletion takes out text the insertion put inside the comment.
    let ops = [insert(6, "XX"), delete(5, "uXXi")];
    let (text, position, quote) = comment_through("The quick fox", 4, "quick", &ops);
    assert_eq!(text, "The qck fox");
    assert_eq!((position, quote.as_str()), (4, "qck"));
}

#[test]
fn comment_quotes_are_edited_in_utf16_units() {
    // "é" is one unit, "𝛼" two: Overleaf counts positions in UTF-16.
    let (_, position, quote) = comment_through("a 𝛼 bé c", 2, "𝛼 bé", &[delete(2, "𝛼 ")]);
    assert_eq!((position, quote.as_str()), (2, "bé"));
    let (_, position, quote) = comment_through("a 𝛼 bé c", 2, "𝛼 bé", &[insert(4, "x")]);
    assert_eq!((position, quote.as_str()), (2, "𝛼x bé"));
}

#[test]
fn suggested_deletion_records_the_removed_text_at_its_point() {
    // The report's 'quick ' case.
    let (text, changes) = edit("The quick brown fox", &[], &[delete(4, "quick ")], Some(&ada()));
    assert_eq!(text, "The brown fox");
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    assert_eq!((change.position, change.text.as_str(), change.deletion), (4, "quick ", true));
    assert_eq!((change.user_id.as_deref(), change.hue), (Some("ada"), 30));
    assert!(is_provisional(change));
}

#[test]
fn suggested_typing_extends_the_suggesters_own_insertion() {
    let (text, changes) = edit("The fox", &[], &[insert(4, "qu")], Some(&ada()));
    assert_eq!(shown(&changes), ["+4:qu"]);
    let (text, changes) = edit(&text, &changes, &[insert(6, "ick ")], Some(&ada()));
    assert_eq!(shown(&changes), ["+4:quick "]);
    // Someone else typing at its edge starts their own.
    let (_, changes) = edit(&text, &changes, &[insert(10, "brown ")], Some(&grace()));
    assert_eq!(shown(&changes), ["+10:brown ", "+4:quick "]);
}

#[test]
fn a_suggestion_inside_someone_elses_insertion_splits_it() {
    let (_, changes) =
        edit("The quick fox", &[insertion(4, "quick ")], &[insert(6, "ZZ")], Some(&ada()));
    assert_eq!(shown(&changes), ["+4:qu", "+6:ZZ", "+8:ick "]);
    let rest = changes.iter().find(|change| change.text == "ick ").unwrap();
    assert_eq!(rest.user_id.as_deref(), Some("grace"));
}

#[test]
fn deleting_suggested_text_again_withdraws_it() {
    let own = change(4, "quick ", &ada(), "ins", false);
    let (text, changes) =
        edit("The quick brown fox", &[own], &[delete(2, "e quick b")], Some(&ada()));
    assert_eq!(text, "Thrown fox");
    // Only the text that was really in the document becomes a deletion.
    assert_eq!(shown(&changes), ["-2:e b"]);
}

#[test]
fn backspacing_and_forward_deleting_read_as_one_deletion_in_order() {
    let mut state = ("The quick brown fox".to_string(), Vec::new());
    for op in [delete(8, "k"), delete(7, "c"), delete(4, "qui")] {
        state = edit(&state.0, &state.1, &[op], Some(&ada()));
    }
    assert_eq!(shown(&state.1), ["-4:quick"]);
    for op in [delete(4, " "), delete(4, "brown")] {
        state = edit(&state.0, &state.1, &[op], Some(&ada()));
    }
    assert_eq!(shown(&state.1), ["-4:quick brown"]);
}

#[test]
fn another_suggesters_deletion_stays_separate() {
    let (_, changes) =
        edit("The brown fox", &[deletion(4, "quick ")], &[delete(3, " brown")], Some(&ada()));
    assert_eq!(shown(&changes), ["-3: brown", "-3:quick "]);
    let theirs = changes.iter().find(|change| change.text == "quick ").unwrap();
    assert_eq!(theirs.id, "del");
}

#[test]
fn each_op_of_a_composed_operation_applies_to_what_the_last_left() {
    let (text, changes) = edit("abcdef", &[], &[insert(1, "XY"), delete(5, "de")], Some(&ada()));
    assert_eq!(text, "aXYbcf");
    assert_eq!(shown(&changes), ["+1:XY", "-5:de"]);
}

#[test]
fn positions_count_utf16_units() {
    // "é" is one unit, "🔵" two: an insertion after the emoji starts at 3.
    let added = [change(3, "ab", &grace(), "ins", false)];
    let (_, changes) = edit("é🔵ab!", &added, &[insert(0, "🔵")], None);
    assert_eq!(shown(&changes), ["+5:ab"]);
}
