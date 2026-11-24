//! The launches after the first pass: a writer who switched back to the
//! Tauri app and returns ([`catch_up`], [`recent_projects_union`]).

use super::*;

/// Write `value` under `key` in the Tauri app's localStorage, as the Tauri
/// app does while it runs (after a switch back to it).
fn store(website_data: &Path, key: &str, value: &str) {
    let path = website_data.join("Default/salt/salt/LocalStorage/localstorage.sqlite3");
    // A write in the same millisecond as the last one would leave its time
    // unchanged.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let connection = rusqlite::Connection::open(&path).unwrap();
    let blob: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
    connection
        .execute(
            "INSERT INTO ItemTable VALUES (?1, ?2)",
            rusqlite::params![key, blob],
        )
        .unwrap();
    std::mem::forget(connection);
}

fn project(name: &str, opened_at: Option<u64>) -> Value {
    let mut entry = json!({ "name": name, "path": format!("/p/{name}") });
    if let Some(opened_at) = opened_at {
        entry["openedAt"] = opened_at.into();
    }
    entry
}

fn union(own: &[Value], tauri: &[Value]) -> (usize, Vec<Value>) {
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = scratch(&format!("union-{run}"));
    let target = dir.join("recent-projects.json");
    if !own.is_empty() {
        write(&target, &pretty(&Value::Array(own.to_vec())));
    }
    let changed =
        recent_projects_union(Some(&pretty(&Value::Array(tauri.to_vec()))), &target).unwrap();
    let list = std::fs::read(&target)
        .ok()
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    (changed, list)
}

#[test]
fn a_later_switch_joins_both_lists_newest_first_in_either_order() {
    // The Tauri app's projects newer than the GPUI app's.
    let (changed, list) = union(
        &[project("g1", Some(20)), project("g2", Some(10))],
        &[project("t1", Some(40)), project("t2", Some(30))],
    );
    assert_eq!(changed, 2);
    assert_eq!(
        list,
        vec![
            project("t1", Some(40)),
            project("t2", Some(30)),
            project("g1", Some(20)),
            project("g2", Some(10)),
        ]
    );
    // And older: the same list either way.
    let (_, list) = union(
        &[project("t1", Some(40)), project("t2", Some(30))],
        &[
            project("g1", Some(20)),
            project("g2", Some(10)),
            project("old", None),
        ],
    );
    assert_eq!(
        list,
        vec![
            project("t1", Some(40)),
            project("t2", Some(30)),
            project("g1", Some(20)),
            project("g2", Some(10)),
            project("old", None),
        ]
    );
}

#[test]
fn a_folder_in_both_lists_keeps_the_newer_opening() {
    let (changed, list) = union(
        &[project("a", Some(10)), project("b", Some(50))],
        &[project("a", Some(60)), project("b", Some(5))],
    );
    assert_eq!(changed, 1);
    assert_eq!(list, vec![project("a", Some(60)), project("b", Some(50))]);
    // Nothing newer: the list is left as it was, unwritten.
    let (changed, list) = union(
        &[project("a", Some(60)), project("b", Some(50))],
        &[project("a", Some(60)), project("b", None)],
    );
    assert_eq!(changed, 0);
    assert_eq!(list, vec![project("a", Some(60)), project("b", Some(50))]);
}

#[test]
fn the_union_stops_at_the_cap_and_never_drops_the_gpui_apps_own() {
    let own: Vec<Value> = (0..RECENT_PROJECTS_KEPT as u64 - 2)
        .map(|n| project(&format!("g{n}"), Some(n)))
        .collect();
    let tauri: Vec<Value> = (0..5)
        .map(|n| project(&format!("t{n}"), Some(100 + n)))
        .collect();
    let (_, list) = union(&own, &tauri);
    assert_eq!(list.len(), RECENT_PROJECTS_KEPT);
    // The two newest of the Tauri app's, then all of the GPUI app's.
    assert_eq!(list[0], project("t4", Some(104)));
    assert_eq!(list[1], project("t3", Some(103)));
    for entry in &own {
        assert!(list.contains(entry), "{entry} was dropped");
    }
    // A full list takes none of the Tauri app's in, however new.
    let full: Vec<Value> = (0..RECENT_PROJECTS_KEPT as u64)
        .map(|n| project(&format!("g{n}"), Some(n)))
        .collect();
    let (changed, list) = union(&full, &tauri);
    assert_eq!(changed, 0);
    assert_eq!(list.len(), RECENT_PROJECTS_KEPT);
    for entry in &full {
        assert!(list.contains(entry));
    }
}

#[test]
fn a_launch_after_the_tauri_app_ran_again_catches_up_on_what_changed_there() {
    let profile = profile("later");
    full_tauri_profile(&profile);
    let (tauri, gpui) = (&profile.tauri, &profile.gpui);
    let settings = crate::file_in(&gpui.state);
    // No marker yet: the first pass's to do, not this.
    assert!(catch_up(tauri, gpui).is_none());
    assert!(migrate(tauri, gpui).is_some());
    // The first read imports the Tauri app's settings, as the app does.
    let website_data = tauri.website_data.as_deref().unwrap();
    assert!(matches!(
        crate::load(&settings, Some(website_data)),
        crate::Loaded::Imported(_)
    ));
    // The launch after the first pass: the Tauri app has not run since.
    assert!(catch_up(tauri, gpui).is_none());

    // Some time in the GPUI app...
    write(
        &gpui.state.join("recent-projects.json"),
        r#"[{"name":"Native","path":"/Users/ada/Native","openedAt":1800000000000},{"name":"Thesis","path":"/Users/ada/Thesis","openedAt":1750000000000}]"#,
    );
    write(&gpui.state.join("recent-commands.json"), r#"["mine"]"#);
    write(
        &gpui.overleaf.join("overleaf-session.json"),
        r#"{"cookie":"mine"}"#,
    );
    let mut own = crate::read(&settings).unwrap();
    own.author_name = "Ada in native".into();
    crate::write(&settings, &own).unwrap();
    // ...then back in the Tauri app, which opens projects, changes a
    // setting and adds to the rest of its state.
    store(
        website_data,
        RECENT_PROJECTS,
        r#"[{"name":"Back","path":"/Users/ada/Back","openedAt":1900000000000},{"name":"Thesis","path":"/Users/ada/Thesis","openedAt":1850000000000}]"#,
    );
    store(website_data, RECENT_COMMANDS, r#"["tauri"]"#);
    store(website_data, "lattice.theme-preference.v1", "light");
    write(
        &tauri.config.join("overleaf-session.json"),
        r#"{"cookie":"tauri"}"#,
    );
    // Signed in again there, after the GPUI app's sign-in; the download
    // folder the GPUI app picked last stays.
    age(&gpui.overleaf.join("overleaf-session.json"), 120);
    write(
        &tauri.config.join("overleaf-download-folder.json"),
        r#"{"folder":"/Users/ada/Older"}"#,
    );
    age(&tauri.config.join("overleaf-download-folder.json"), 120);
    write(
        &gpui.overleaf.join("overleaf-download-folder.json"),
        r#"{"folder":"/Users/ada/Mine"}"#,
    );
    write(
        &tauri.config.join("overleaf-local-copies.json"),
        r#"{"folders":["/Users/ada/Overleaf Projects/Thesis","/Users/ada/Paper","/Users/ada/Grant"]}"#,
    );
    write(
        &tauri.data.join("themes/dev.ada.new.json"),
        r#"{"id":"dev.ada.new"}"#,
    );
    write(&tauri.data.join("synara/skills/cite/SKILL.md"), "# Cite");
    let tauri_before = snapshot(&profile.root.join("tauri"));

    let report = catch_up(tauri, gpui).expect("the Tauri app ran since");
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.carried["recent projects"], 2);
    assert_eq!(report.carried["settings"], 1);
    assert_eq!(report.carried["themes"], 1);
    assert_eq!(report.carried["skills"], 1);
    assert_eq!(report.carried["overleaf linked folders"], 1);
    assert_eq!(
        read_json(&gpui.state.join("recent-projects.json")),
        json!([
            { "name": "Back", "path": "/Users/ada/Back", "openedAt": 1900000000000u64 },
            { "name": "Thesis", "path": "/Users/ada/Thesis", "openedAt": 1850000000000u64 },
            { "name": "Native", "path": "/Users/ada/Native", "openedAt": 1800000000000u64 }
        ])
    );
    // The setting changed there wins; the one changed only here stays.
    let caught_up = crate::read(&settings).unwrap();
    assert_eq!(caught_up.theme_preference, crate::ThemePreference::Light);
    assert_eq!(caught_up.author_name, "Ada in native");
    assert!(gpui.data.join("themes/dev.ada.new.json").exists());
    assert!(gpui.data.join("synara/skills/cite/SKILL.md").exists());
    assert_eq!(
        read_json(&gpui.overleaf.join("overleaf-local-copies.json"))["folders"][2],
        "/Users/ada/Grant"
    );
    // What the GPUI app already has stays as it is.
    assert_eq!(
        read_json(&gpui.state.join("recent-commands.json")),
        json!(["mine"])
    );
    // Of the Overleaf files both apps have, the newer one.
    assert_eq!(report.carried["overleaf session"], 1);
    assert_eq!(
        read_json(&gpui.overleaf.join("overleaf-session.json")),
        json!({ "cookie": "tauri" })
    );
    assert_eq!(
        read_json(&gpui.overleaf.join("overleaf-download-folder.json")),
        json!({ "folder": "/Users/ada/Mine" })
    );
    // The Tauri app's files are left exactly as they were.
    assert_eq!(snapshot(&profile.root.join("tauri")), tauri_before);

    // Once per run of the Tauri app: the next launch finds the storage as it
    // was, and a setting changed here afterwards is not undone.
    assert!(catch_up(tauri, gpui).is_none());
    let mut own = crate::read(&settings).unwrap();
    own.theme_preference = crate::ThemePreference::Dark;
    crate::write(&settings, &own).unwrap();
    store(website_data, RECENT_COMMANDS, r#"["again"]"#);
    let report = catch_up(tauri, gpui).expect("the Tauri app ran again");
    assert_eq!(report.carried.get("settings"), None);
    assert_eq!(
        crate::read(&settings).unwrap().theme_preference,
        crate::ThemePreference::Dark
    );
    let _ = std::fs::remove_dir_all(&profile.root);
}

#[test]
fn a_marker_from_before_0_20_takes_every_setting_the_tauri_app_holds() {
    let profile = profile("old-marker");
    full_tauri_profile(&profile);
    let (tauri, gpui) = (&profile.tauri, &profile.gpui);
    assert!(migrate(tauri, gpui).is_some());
    // A 0.19 marker: no settings kept.
    let marker = gpui.state.join(MARKER);
    let mut body = read_json(&marker);
    body.as_object_mut().unwrap().remove("settings");
    write(&marker, &pretty(&body));
    let settings = crate::file_in(&gpui.state);
    crate::write(&settings, &crate::Settings::default()).unwrap();
    store(
        tauri.website_data.as_deref().unwrap(),
        "lattice.author-name.v1",
        "Ada",
    );
    let report = catch_up(tauri, gpui).unwrap();
    // The theme the Tauri app held all along (dark) and the name set there.
    assert_eq!(report.carried["settings"], 2);
    let caught_up = crate::read(&settings).unwrap();
    assert_eq!(caught_up.theme_preference, crate::ThemePreference::Dark);
    assert_eq!(caught_up.author_name, "Ada");
    let _ = std::fs::remove_dir_all(&profile.root);
}

/// Hand test B (LATA-87): the name set in the GPUI app under 0.19, then
/// back to the Tauri app, whose stale name must not replace it.
#[test]
fn a_marker_from_before_0_20_keeps_what_the_gpui_app_changed_from_its_defaults() {
    let profile = profile("old-marker-switched-back");
    full_tauri_profile(&profile);
    let (tauri, gpui) = (&profile.tauri, &profile.gpui);
    let website_data = tauri.website_data.as_deref().unwrap();
    store(website_data, "lattice.author-name.v1", "Ada in classic");
    assert!(migrate(tauri, gpui).is_some());
    let marker = gpui.state.join(MARKER);
    let mut body = read_json(&marker);
    body.as_object_mut().unwrap().remove("settings");
    write(&marker, &pretty(&body));
    let settings = crate::file_in(&gpui.state);
    let own = crate::Settings {
        author_name: "Ada in native".into(),
        theme_preference: crate::ThemePreference::Light,
        ..Default::default()
    };
    crate::write(&settings, &own).unwrap();
    // Back in the Tauri app, which still holds its old name and dark theme
    // and now wraps History's diffs.
    assert!(!crate::Settings::default().history_diff_wrap);
    store(website_data, "lattice.history.diff-wrap.v1", "true");

    let report = catch_up(tauri, gpui).unwrap();
    let caught_up = crate::read(&settings).unwrap();
    assert_eq!(caught_up.author_name, "Ada in native");
    assert_eq!(caught_up.theme_preference, crate::ThemePreference::Light);
    assert!(caught_up.history_diff_wrap);
    assert_eq!(report.carried["settings"], 1);
    let _ = std::fs::remove_dir_all(&profile.root);
}
