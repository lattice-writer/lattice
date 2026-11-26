//! The heartbeat watchdog and the wake, against the mock server: a socket
//! gone quiet is reported lost, and so is one the computer slept on.

use super::client::{awake_or_asleep, lock};
use super::mock_server::*;
use super::tests::{connect, wait_for};
use super::*;
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
