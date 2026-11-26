//! The heartbeat watchdog and the wake, against the mock server: a socket
//! gone quiet is reported lost, and so is one the computer slept on, and an
//! answer that never comes ends the connection only on a silent socket.

use super::client::{awake_or_asleep, lock};
use super::mock_server::*;
use super::tests::{connect, wait_for};
use super::*;
use crate::runtime as rt;
use std::time::{Duration, Instant};

#[test]
fn a_server_that_goes_quiet_past_its_heartbeat_timeout_is_reported_lost() {
    // The mock never sends a heartbeat after joining: a half-open socket.
    let (host, _state) = start_mock_with(false, 1);
    let (_client, events) = connect(mock_config(&host, "overleaf_session2=test-cookie"));
    let lost = wait_for(&events, 5, |event| match event {
        RealtimeEvent::Disconnected { reason } => Some(reason.clone()),
        _ => None,
    });
    assert!(lost.is_some_and(|reason| reason.contains("sent nothing")), "{:?}", lock(&events));
}

#[test]
fn the_watchdogs_clock_keeps_awake_time_too() {
    // It also counts sleep, which a test cannot make happen; what it must
    // not do is run slow or stand still while the computer is awake.
    let (before, started) = (awake_or_asleep(), Instant::now());
    std::thread::sleep(Duration::from_millis(200));
    let moved = awake_or_asleep().saturating_sub(before);
    let bound = started.elapsed() + Duration::from_millis(50);
    assert!(moved >= Duration::from_millis(195) && moved <= bound, "{moved:?}");
}

#[test]
fn waking_from_sleep_reports_the_connection_lost_at_once() {
    // The mock names a minute of heartbeat timeout, and the socket is still
    // open: only the wake can end it this soon.
    let (host, _state) = start_mock(false);
    let (client, events) = connect(mock_config(&host, "overleaf_session2=test-cookie"));
    client.rejoin_after_sleep();
    let lost = wait_for(&events, 2, |event| match event {
        RealtimeEvent::Disconnected { reason } => Some(reason.clone()),
        _ => None,
    });
    assert!(lost.is_some_and(|reason| reason.contains("slept")), "{:?}", lock(&events));
    assert!(client.current_entities().is_none());
}

/// An update left unanswered on a socket that stays `heard`: the send's
/// error, and whether the connection was reported lost.
fn unanswered_update(heard: Heard) -> (String, Option<String>, bool) {
    let (host, state) = start_mock(false);
    let (client, events) = connect(mock_config(&host, "overleaf_session2=test-cookie"));
    rt::block_on(client.join_doc("doc-1", None)).expect("join");
    // What the mock sends after the join, an update and then a heartbeat,
    // arrives before the update goes: the echo is the heartbeat heard.
    wait_for(&events, 2, |event| matches!(event, RealtimeEvent::DocUpdate { .. }).then_some(()));
    let echoed = (0..40).any(|_| {
        std::thread::sleep(Duration::from_millis(50));
        lock(&state).frames.iter().any(|frame| frame == "2::")
    });
    assert!(echoed, "the mock's heartbeat was never echoed");
    leave_updates_unanswered(&state, heard);
    let op = OtOp { p: 0, i: Some("x".into()), d: None, u: None };
    let sent = rt::block_on(client.send_ops("doc-1", 43, vec![op], false, Submission::default()));
    let lost = wait_for(&events, 1, |event| match event {
        RealtimeEvent::Disconnected { reason } => Some(reason.clone()),
        _ => None,
    });
    let live = client.current_entities().is_some();
    client.shutdown();
    (sent.expect_err("no answer came"), lost, live)
}

#[test]
fn a_join_ask_the_server_never_answers_ends_nothing() {
    // A server that pushes the project ignores the `joinProject` ask, and
    // this mock sends nothing after the join: a quiet but healthy socket.
    let (host, _state) = start_mock(true);
    let (client, events) = connect(mock_config(&host, "overleaf_session2=test-cookie"));
    std::thread::sleep(Duration::from_secs(11));
    let lost =
        lock(&events).iter().any(|event| matches!(event, RealtimeEvent::Disconnected { .. }));
    assert!(!lost && client.current_entities().is_some(), "{:?}", lock(&events));
    client.shutdown();
}

#[test]
fn an_answer_that_never_comes_on_a_silent_socket_ends_the_connection() {
    let (error, lost, live) = unanswered_update(Heard::Nothing);
    assert!(lost.is_some_and(|reason| reason.contains("sent nothing since")), "{error}");
    assert!(!live, "{error}");
}

#[test]
fn an_answer_that_never_comes_on_a_socket_still_heard_from_is_only_a_timeout() {
    let (error, lost, live) = unanswered_update(Heard::Heartbeats);
    assert!(error.contains("timed out"), "{error}");
    assert!(lost.is_none() && live, "{lost:?}");
}

/// A network change probed on a socket that answers it, or leaves it
/// unanswered while still `heard`: why it was reported lost, if it was, and
/// when.
fn probed_path_change(heard: Option<Heard>) -> (Option<String>, Duration) {
    // The mock names a minute of heartbeat timeout and sends none of its
    // own, so the probe's answer is all a healthy socket hears.
    let (host, state) = start_mock(false);
    let (client, events) = connect(mock_config(&host, "overleaf_session2=test-cookie"));
    if let Some(heard) = heard {
        leave_updates_unanswered(&state, heard);
    }
    let probed = Instant::now();
    client.probe_after_path_change();
    let lost = wait_for(&events, PATH_PROBE.as_secs() + 2, |event| match event {
        RealtimeEvent::Disconnected { reason } => Some(reason.clone()),
        _ => None,
    });
    (lost, probed.elapsed())
}

#[test]
fn a_network_change_on_a_socket_that_answers_the_probe_keeps_the_connection() {
    let (lost, _) = probed_path_change(None);
    assert!(lost.is_none(), "{lost:?}");
}

#[test]
fn a_network_change_on_a_silent_socket_reports_it_lost_once_the_probe_goes_unanswered() {
    let (lost, after) = probed_path_change(Some(Heard::Nothing));
    assert!(lost.is_some_and(|reason| reason.contains("network changed")), "{after:?}");
    assert!(after >= PATH_PROBE && after < PATH_PROBE + Duration::from_secs(1), "{after:?}");
}

#[test]
fn a_network_change_that_leaves_only_the_way_back_working_reports_it_lost() {
    // Overleaf's heartbeats still arrive; the probe's answer never does.
    let (lost, after) = probed_path_change(Some(Heard::Heartbeats));
    assert!(lost.is_some_and(|reason| reason.contains("network changed")), "{after:?}");
}
