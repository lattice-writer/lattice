//! Lattice's own writes in Overleaf's history: an upload or a live edit
//! Lattice made itself never vouches for a hollow copy of the same file,
//! while a co-author's later edit to it still does.

use super::*;

/// Lattice's own upload lands in Overleaf's history like anyone's edit, and
/// leaves the next sync without a fresh version to compare against. Neither
/// may let that upload vouch for a hollow copy of the same file; a later edit
/// to it on Overleaf still does.
#[test]
fn overleaf_history_never_counts_lattices_own_upload_as_confirmation() {
    let notes = "Notes kept only in this project.\n".repeat(60);
    let edited = format!("{notes}One more line written here.\n");
    let now = chrono::Utc::now().timestamp_millis();
    let upload = json!({ "fromV": 40, "toV": 41, "meta": { "end_ts": now },
        "pathnames": ["notes.md"] });
    let base: Files = &[("notes.md", notes.as_bytes())];
    let pushing = Mock { versions: vec![40, 40, 41], ..Mock::project(base) }.serve();
    let (config, root) = linked(&pushing, &[("notes.md", edited.as_bytes())], base);
    let result = sync(&config, &root, NO_LIVE, None).unwrap();
    assert_eq!(result.pushed, ["notes.md"]);
    let state = load_state(&root).unwrap();
    assert_eq!((state.remote_version, state.agreed_version), (None, Some(40)));

    let resync = |history: Vec<Value>| {
        let mock = Mock { versions: vec![41], history, ..Mock::project(&[("notes.md", b"")]) };
        let server = mock.serve();
        let config = signed_in(&server.base);
        edit_state(&root, |state| state.host = server.base.clone());
        sync(&config, &root, NO_LIVE, None).unwrap()
    };
    let result = resync(vec![upload.clone()]);
    assert_eq!(result.refused_incoming, ["notes.md"]);
    assert_eq!(read_local(&root, "notes.md").as_deref(), Some(edited.as_bytes()));

    let emptied = json!({ "fromV": 41, "toV": 42, "meta": { "end_ts": now + 60_000 },
        "pathnames": ["notes.md"] });
    let result = resync(vec![emptied, upload]);
    assert_eq!(result.pulled, ["notes.md"]);
    assert_eq!(read_local(&root, "notes.md").as_deref(), Some(b"".as_slice()));
}

/// Edits sent from Lattice over the realtime channel are already in the copy
/// it checkpoints when it leaves the document, so they cannot vouch for a
/// hollow download of that document either; a collaborator's later edit can.
#[test]
fn overleaf_history_never_counts_lattices_own_realtime_edits_as_confirmation() {
    let notes = "Notes kept only in this project.\n".repeat(60);
    let typed = format!("{notes}Typed in Lattice while the document was open.\n");
    let now = chrono::Utc::now().timestamp_millis();
    let typing = json!({ "fromV": 40, "toV": 41, "meta": { "end_ts": now - 60_000 },
        "pathnames": ["notes.md"] });
    let base: Files = &[("notes.md", notes.as_bytes())];
    let run = |history: Vec<Value>| {
        let mock = Mock { versions: vec![41], history, ..Mock::project(&[("notes.md", b"")]) };
        let server = mock.serve();
        let (config, root) = linked(&server, &[("notes.md", typed.as_bytes())], base);
        edit_state(&root, |state| state.remote_version = Some(40));
        checkpoint_realtime_text(&root, "notes.md", &typed).unwrap();
        let result = sync(&config, &root, NO_LIVE, None).unwrap();
        (root, result)
    };

    let (root, result) = run(vec![typing.clone()]);
    assert_eq!(result.refused_incoming, ["notes.md"]);
    assert_eq!(read_local(&root, "notes.md").as_deref(), Some(typed.as_bytes()));

    let emptied = json!({ "fromV": 41, "toV": 42, "meta": { "end_ts": now + 3_600_000 },
        "pathnames": ["notes.md"] });
    let (root, result) = run(vec![emptied, typing]);
    assert_eq!(result.pulled, ["notes.md"]);
    assert_eq!(read_local(&root, "notes.md").as_deref(), Some(b"".as_slice()));
}
