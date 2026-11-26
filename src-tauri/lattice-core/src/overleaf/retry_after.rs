//! A 429's `Retry-After`, in whole seconds. lattice-core waits out a short
//! one itself (`api::retry_wait`); a longer one ends the request, and its
//! error says "; retry after N s" (`api::expect_success`, an upload's
//! failure) so the GPUI app's live mode can hold its syncs that long.

use reqwest::blocking::Response;
use reqwest::header::RETRY_AFTER;

/// The whole seconds a 429 asks to be left alone for, from its
/// `Retry-After`; `None` when it has none, or one this cannot read.
pub(super) fn retry_after(response: &Response) -> Option<u64> {
    let value = response.headers().get(RETRY_AFTER)?.to_str().ok()?;
    parse_retry_after(value, super::clock::now_ms(response.url().as_str()))
}

/// `Retry-After` is a count of seconds or an HTTP date (RFC 9110 §10.2.3).
/// The date is Overleaf's clock, so it is measured from Overleaf's now
/// (`overleaf_now_ms`, see [`super::clock`]), not this Mac's, which may be
/// minutes off; a date already past asks for no wait. Only the IMF-fixdate
/// form is read ("Sun, 06 Nov 1994 08:49:37 GMT", what servers send);
/// the obsolete RFC 850 and asctime forms read as none.
fn parse_retry_after(value: &str, overleaf_now_ms: i64) -> Option<u64> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds);
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let wait_ms = date.timestamp_millis().saturating_sub(overleaf_now_ms);
    Some(u64::try_from(wait_ms).unwrap_or(0).div_ceil(1000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overleaf::api::{expect_success, http_client, send};

    #[test]
    fn retry_after_reads_seconds_and_a_date_on_overleafs_clock() {
        assert_eq!(parse_retry_after(" 45 ", 0), Some(45));
        assert_eq!(parse_retry_after("0", 0), Some(0));
        // 1994-11-06T08:49:37Z, read against Overleaf's now, rounded up.
        let date = "Sun, 06 Nov 1994 08:49:37 GMT";
        let at = 784_111_777_000;
        assert_eq!(parse_retry_after(date, at - 90_000), Some(90));
        assert_eq!(parse_retry_after(date, at - 89_400), Some(90));
        // Already past on Overleaf's clock: no wait asked for.
        assert_eq!(parse_retry_after(date, at + 5_000), Some(0));
        for unreadable in ["", "soon", "-5", "1.5", "Sunday, 06-Nov-94 08:49:37 GMT"] {
            assert_eq!(parse_retry_after(unreadable, at), None, "{unreadable}");
        }
    }

    #[test]
    fn a_rate_limited_answer_says_when_to_retry() {
        let base = crate::overleaf_rt::mock_server::serve_http(|request| {
            let response =
                tiny_http::Response::from_string("Too Many Requests").with_status_code(429);
            let response = match request.url().ends_with("/told") {
                true => {
                    response.with_header("Retry-After: 45".parse::<tiny_http::Header>().unwrap())
                }
                false => response,
            };
            let _ = request.respond(response);
        });
        let client = http_client(10).unwrap();
        // A POST is never resent, so each answer comes straight back.
        let told = send(client.post(format!("{base}/told")).body("{}")).unwrap();
        assert_eq!(
            expect_success(told, "for the project download").unwrap_err(),
            "Overleaf returned 429 Too Many Requests for the project download; retry after 45 s."
        );
        let untold = send(client.post(format!("{base}/untold")).body("{}")).unwrap();
        assert_eq!(
            expect_success(untold, "for the project download").unwrap_err(),
            "Overleaf returned 429 Too Many Requests for the project download."
        );
    }
}
