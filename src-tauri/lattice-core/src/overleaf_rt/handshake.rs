//! The Socket.IO 0.9 handshake that comes before the websocket: the session
//! id, the heartbeat timeout, and the cookies the upgrade must carry.

use super::client::now_millis;
use super::codec::parse_handshake;
use super::{SESSION_EXPIRED, USER_AGENT};
use crate::util::url_encode;
use std::collections::HashMap;
use std::time::Duration;

/// What the handshake settled: the session id, the heartbeat timeout in
/// seconds (0 = none), and the cookies to carry into the websocket upgrade. Runs on a blocking thread because the crate only has
/// `reqwest`'s blocking client.
pub(super) fn handshake_blocking(
    origin: &str, cookie: &str, project_id: &str,
) -> Result<(String, u64, String), String> {
    let url =
        format!("{origin}/socket.io/1/?projectId={}&t={}", url_encode(project_id), now_millis());
    let client = crate::http::blocking_as(USER_AGENT, Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Could not build the HTTP client: {e}"))?;
    let response = client
        .get(&url)
        .header("Cookie", cookie)
        .header("Origin", origin)
        .send()
        .map_err(|e| format!("Could not reach Overleaf ({e})."))?;
    let status = response.status();
    let final_url = response.url().to_string();
    let handed_back: Vec<String> = (response.headers().get_all(reqwest::header::SET_COOKIE).iter())
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect();
    let body = response
        .text()
        .map_err(|e| format!("Could not read the Overleaf handshake response: {e}"))?;

    if status.as_u16() == 401 || status.as_u16() == 403 || final_url.contains("/login") {
        return Err(SESSION_EXPIRED.to_string());
    }
    if !status.is_success() {
        return Err(format!("Overleaf refused the realtime handshake (HTTP {}).", status.as_u16()));
    }
    let login_page = body.trim_start().starts_with('<');
    let (sid, heartbeat) =
        parse_handshake(&body)
            .map_err(|e| if login_page { SESSION_EXPIRED.to_string() } else { e })?;
    Ok((sid, heartbeat, merge_cookies(cookie, &handed_back)))
}

/// Fold any cookies the handshake set into the ones we already had.
///
/// Overleaf runs several realtime instances behind a load balancer, and the
/// handshake answers with a cookie that pins the session to the instance
/// holding it. Leaving that cookie behind means the websocket upgrade reaches
/// a different instance, which has never heard of the id we were just given
/// and answers 502 — a failure that looks like the server being down.
///
/// Later values win, but a cookie keeps the position it first appeared in, so
/// the header stays stable and readable across reconnects.
pub(super) fn merge_cookies(base: &str, set_cookies: &[String]) -> String {
    let mut order: Vec<String> = Vec::new();
    let mut values: HashMap<String, String> = HashMap::new();
    let handed_back = set_cookies.iter().filter_map(|header| header.split(';').next());
    for pair in base.split(';').chain(handed_back).map(str::trim) {
        let Some(name) = pair.split_once('=').map(|(name, _)| name.trim()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        if !values.contains_key(name) {
            order.push(name.to_string());
        }
        values.insert(name.to_string(), pair.to_string());
    }
    order.iter().filter_map(|name| values.get(name)).cloned().collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
    use super::merge_cookies;

    #[test]
    fn cookies_from_the_handshake_join_the_ones_we_had() {
        // A new cookie is appended, an existing one is replaced in place, and
        // anything without a name is ignored.
        let handed_back =
            ["ol-affinity=instance-7; Path=/; HttpOnly", "other=2; Path=/", "; Path=/"];
        assert_eq!(
            merge_cookies("overleaf_session2=abc; other=1", &handed_back.map(String::from)),
            "overleaf_session2=abc; other=2; ol-affinity=instance-7"
        );
        assert_eq!(merge_cookies("session=x", &[]), "session=x");
    }
}
