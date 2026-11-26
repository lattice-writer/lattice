//! Overleaf's clock, as its answers tell it. Where Lattice cannot learn a
//! project version it stamps its own writes and syncs with a time, and
//! Overleaf's history places changes by Overleaf's clock (`meta.end_ts`).
//! A Mac whose clock is off by more than the history's slack
//! (`review::HISTORY_SLACK_MS`) would then misplace them: with the Mac
//! behind, Lattice's own upload reads as a co-author's later change, which
//! is what vouches for a hollow download of the same file. So every answer's
//! `Date` header records how far Overleaf's clock is from this one, per
//! host, and those stamps are taken in Overleaf's time.

use reqwest::blocking::Response;
use reqwest::header::DATE;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Overleaf's clock minus this Mac's, in ms, by origin.
static OFFSETS: Mutex<BTreeMap<String, i64>> = Mutex::new(BTreeMap::new());

fn origin(url: &reqwest::Url) -> String {
    url.origin().ascii_serialization()
}

/// Note how far the clock of the host that sent `response` is from ours,
/// under the origin `asked` too when a redirect answered from another one,
/// since a project keeps the host it was asked at.
pub(super) fn observe(response: &Response, asked: Option<&reqwest::Url>) {
    let Some(date) = (response.headers().get(DATE))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| chrono::DateTime::parse_from_rfc2822(value).ok())
    else {
        return;
    };
    let offset = date.timestamp_millis() - chrono::Utc::now().timestamp_millis();
    if let Ok(mut offsets) = OFFSETS.lock() {
        offsets.insert(origin(response.url()), offset);
        if let Some(asked) = asked {
            offsets.insert(origin(asked), offset);
        }
    }
}

/// Now on `host`'s clock, in Unix ms: ours when it has not answered yet.
pub(super) fn now_ms(host: &str) -> i64 {
    let offset = (reqwest::Url::parse(host).ok())
        .and_then(|url| OFFSETS.lock().ok()?.get(&origin(&url)).copied())
        .unwrap_or_default();
    chrono::Utc::now().timestamp_millis() + offset
}
