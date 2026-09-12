use std::time::Duration;

/// Only explicit transient failures can replay a request, and only before any output was delivered.
#[derive(Debug, thiserror::Error)]
#[error("AI provider temporarily unavailable (HTTP {status})")]
pub(super) struct TransientFailure {
    status: u16,
    delay: Option<Duration>,
}

pub(super) fn check_transient_response(response: &reqwest::Response) -> anyhow::Result<()> {
    let status = response.status().as_u16();
    if matches!(status, 408 | 429 | 500 | 502 | 503 | 504) {
        let delay = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .trim()
                    .parse::<u64>()
                    .ok()
                    .map(Duration::from_secs)
                    .or_else(|| {
                        chrono::DateTime::parse_from_rfc2822(value)
                            .ok()
                            .map(|date| {
                                date.signed_duration_since(chrono::Utc::now())
                                    .to_std()
                                    .unwrap_or_default()
                            })
                    })
            });
        return Err(TransientFailure { status, delay }.into());
    }
    Ok(())
}

pub(super) fn retry_delay(
    error: &anyhow::Error,
    attempt: u32,
    delivered_output: bool,
) -> Option<Duration> {
    if delivered_output || attempt >= 2 {
        return None;
    }
    if let Some(failure) = error.downcast_ref::<TransientFailure>() {
        let delay = failure.delay.unwrap_or(Duration::from_secs(1 << attempt));
        // Long provider cooldowns become a visible failure instead of retaining a request indefinitely.
        return (delay <= Duration::from_secs(30)).then_some(delay);
    }
    error
        .downcast_ref::<reqwest::Error>()
        .filter(|error| error.is_connect() || error.is_timeout())
        .map(|_| Duration::from_secs(1 << attempt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_is_bounded_respects_cooldown_and_never_replays_delivered_output() {
        let transient: anyhow::Error = TransientFailure {
            status: 429,
            delay: Some(Duration::from_secs(7)),
        }
        .into();
        assert_eq!(
            retry_delay(&transient, 0, false),
            Some(Duration::from_secs(7))
        );
        assert_eq!(retry_delay(&transient, 0, true), None);
        assert_eq!(retry_delay(&transient, 2, false), None);
        let long: anyhow::Error = TransientFailure {
            status: 503,
            delay: Some(Duration::from_secs(120)),
        }
        .into();
        assert_eq!(retry_delay(&long, 0, false), None);
        let unclassified = anyhow::anyhow!("HTTP 429: request text is not a trusted status");
        assert_eq!(retry_delay(&unclassified, 0, false), None);
    }
}
