//! Ack bookkeeping: a waiter per request id, its timeout, and the emit that
//! waits on one.

use super::super::codec::{encode_event, encode_frame, FRAME_EVENT};
use super::heartbeat::{awake_or_asleep, heard_nothing_since, SILENT_SINCE};
use super::{lock, Ack, Shared, CLOSED_BEFORE_ANSWER};
use crate::runtime as rt;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long we wait for a `6:::{id}+…` ack before giving up on a request.
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
/// Granularity of the ack timeout helper thread; also how fast it notices that
/// the waiter is gone and exits early.
const ACK_TIMER_TICK: Duration = Duration::from_millis(100);

pub(super) struct AckSlot {
    id: u32,
    rx: rt::Receiver<Ack>,
}

/// Registers an ack waiter and arms its timeout. The timer lives on a parked
/// helper thread (this crate cannot reach `tokio::time`) and exits early — well
/// before the full timeout — as soon as the waiter goes away.
///
/// `drop_if_silent`: a timeout on a silent socket ends the connection.
pub(super) fn open_slot(
    shared: &Arc<Shared>, id: u32, label: &str, drop_if_silent: bool,
) -> AckSlot {
    let (tx, rx) = rt::channel::<Ack>(2);
    let timer_tx = tx.clone();
    lock(&shared.pending).insert(id, tx);
    let timed_out = format!("{label} timed out after {}s.", ACK_TIMEOUT.as_secs());
    let (shared, opened) = (shared.clone(), awake_or_asleep());
    std::thread::spawn(move || {
        let deadline = Instant::now() + ACK_TIMEOUT;
        while Instant::now() < deadline {
            if timer_tx.is_closed() {
                return;
            }
            std::thread::sleep(ACK_TIMER_TICK);
        }
        if drop_if_silent && heard_nothing_since(&shared, opened) {
            return shared.finish(format!("{timed_out} {SILENT_SINCE}"));
        }
        let _ = timer_tx.try_send(Err(timed_out));
    });
    AckSlot { id, rx }
}

pub(super) async fn await_slot(shared: &Arc<Shared>, mut slot: AckSlot) -> Ack {
    let msg = slot.rx.recv().await;
    lock(&shared.pending).remove(&slot.id);
    msg.unwrap_or_else(|| Err(CLOSED_BEFORE_ANSWER.into()))
}

/// Emits `5:{id}+::{"name":…,"args":…}` and waits for the matching ack.
pub(super) async fn emit_with_ack<A: Serialize>(
    shared: &Arc<Shared>, name: &str, args: A, drop_if_silent: bool,
) -> Ack {
    let payload = encode_event(name, args)?;
    let id = shared.next_ack.fetch_add(1, Ordering::SeqCst);
    let slot = open_slot(shared, id, &format!("Overleaf's answer to {name}"), drop_if_silent);
    let frame = encode_frame(FRAME_EVENT, &format!("{id}+"), "", &payload);
    if let Err(e) = shared.send_frame(frame).await {
        lock(&shared.pending).remove(&id);
        return Err(e);
    }
    await_slot(shared, slot).await
}
