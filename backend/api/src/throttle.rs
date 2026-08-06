//! How often the same person may guess wrong.
//!
//! Sign-in had nothing at all: a password could be guessed as fast as the
//! network allowed, on every one of the three sign-in pages, for ever. Argon2
//! makes each guess expensive enough to matter, which helps and is not the
//! same thing as stopping.
//!
//! Two keys rather than one, and the difference matters. Counting by address
//! alone is beaten by anybody with a list of addresses; counting by account
//! alone hands a stranger a way to lock somebody out of their own site. So a
//! run of failures against one account pauses that account, a run from one
//! address pauses that address, and either one on its own is enough.
//!
//! Nothing is stored. A restart forgets who was guessing, which is the right
//! trade for a table that would otherwise have to be kept, indexed and swept.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// Wrong answers before the next one is refused without being read.
const BEFORE_PAUSE: u32 = 8;

/// How long the first pause lasts. Each pause after it doubles, up to the
/// longest — someone still guessing after an hour is not somebody who forgot
/// their password.
const FIRST_PAUSE: Duration = Duration::from_secs(60);
const LONGEST_PAUSE: Duration = Duration::from_secs(15 * 60);

/// How long a quiet key is remembered. Long enough that a slow guesser is
/// still counted, short enough that yesterday's typo is not held against
/// anybody.
const REMEMBER: Duration = Duration::from_secs(30 * 60);

/// Above this many keys, the expired ones are dropped. Only a bound on
/// memory: a machine being hammered from a botnet should not be able to grow
/// this without limit.
const SWEEP_ABOVE: usize = 10_000;

#[derive(Debug)]
struct Attempt {
    failures: u32,
    pauses: u32,
    seen_at: Instant,
    paused_until: Option<Instant>,
}

static ATTEMPTS: LazyLock<Mutex<HashMap<String, Attempt>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// How much longer this key has to wait, or `None` if it may try now.
pub fn pause_left(key: &str) -> Option<Duration> {
    let now = Instant::now();
    let attempts = ATTEMPTS.lock().ok()?;

    let paused_until = attempts.get(key)?.paused_until?;
    (paused_until > now).then(|| paused_until - now)
}

/// Counts a wrong answer, and returns how long the key must now wait.
pub fn wrong(key: &str) -> Option<Duration> {
    let now = Instant::now();
    let Ok(mut attempts) = ATTEMPTS.lock() else {
        return None;
    };

    if attempts.len() > SWEEP_ABOVE {
        attempts.retain(|_, attempt| now.duration_since(attempt.seen_at) < REMEMBER);
    }

    let attempt = attempts.entry(key.to_string()).or_insert(Attempt {
        failures: 0,
        pauses: 0,
        seen_at: now,
        paused_until: None,
    });

    // A key nobody has touched in half an hour starts again. Counting for ever
    // would eventually pause somebody who mistypes once a week.
    if now.duration_since(attempt.seen_at) > REMEMBER {
        attempt.failures = 0;
        attempt.pauses = 0;
    }

    attempt.seen_at = now;
    attempt.failures += 1;

    if attempt.failures < BEFORE_PAUSE {
        return None;
    }

    let pause = FIRST_PAUSE
        .saturating_mul(1u32 << attempt.pauses.min(8))
        .min(LONGEST_PAUSE);
    attempt.failures = 0;
    attempt.pauses += 1;
    attempt.paused_until = Some(now + pause);

    Some(pause)
}

/// Forgets a key. Called when somebody gets in: the failures before a correct
/// password were a person forgetting one, not an attack.
pub fn right(key: &str) {
    if let Ok(mut attempts) = ATTEMPTS.lock() {
        attempts.remove(key);
    }
}

/// The address a request came from, as well as it can be known.
///
/// Behind a proxy the socket is the proxy, so the forwarded header is what
/// says who asked — and the *last* entry of it, not the first: the proxy
/// appends the address it saw, so anything before that is whatever the client
/// chose to claim.
pub fn caller(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_of_wrong_answers_ends_in_a_pause() {
        let key = "test:a-run";
        for _ in 1..BEFORE_PAUSE {
            assert!(wrong(key).is_none(), "not yet");
        }
        assert!(wrong(key).is_some(), "the last one pauses");
        assert!(pause_left(key).is_some());
        right(key);
        assert!(pause_left(key).is_none(), "getting in clears it");
    }

    #[test]
    fn each_pause_is_longer_than_the_one_before() {
        let key = "test:doubling";
        let mut pauses = Vec::new();
        for _ in 0..3 {
            for _ in 1..BEFORE_PAUSE {
                wrong(key);
            }
            pauses.push(wrong(key).expect("a pause"));
        }
        assert!(pauses[1] > pauses[0]);
        assert!(pauses[2] > pauses[1]);
        assert!(pauses.iter().all(|pause| *pause <= LONGEST_PAUSE));
        right(key);
    }

    #[test]
    fn the_caller_is_the_address_the_proxy_saw_not_the_one_it_was_told() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "10.9.9.9, 203.0.113.7".parse().unwrap());
        assert_eq!(caller(&headers), "203.0.113.7");

        assert_eq!(caller(&axum::http::HeaderMap::new()), "unknown");
    }
}
