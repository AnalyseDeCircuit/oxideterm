use regex::Regex;
use std::sync::LazyLock;
use zeroize::Zeroizing;

static SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r#"(?ix)(?:--?[\w-]*(?:password|passwd|passphrase|secret|token|api[-_]?key|private[-_]?key|access[-_]?key|authorization|cookie|credential)[\w-]*[=\s]+|\b[\w-]*(?:password|passwd|passphrase|secret|token|api[-_]?key|private[-_]?key|access[-_]?key|authorization|cookie|credential)[\w-]*[\"']?\s*[:=]\s*|(?:proxy-)?authorization\s*:|cookie\s*:|-----BEGIN\s+(?:\w+\s+)?PRIVATE\s+KEY-----)"#
).expect("audit secret pattern")
});
static URI_PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([a-z][a-z0-9+.-]*://[^\s/:@]+:)[^\s/@]+(@)").expect("audit URI pattern")
});
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-(?:proj-|ant-)?[A-Za-z0-9_-]{16,}|xox[bpoas]-[A-Za-z0-9-]{10,}|AKIA[A-Z0-9]{16})\b"
).expect("audit token pattern")
});
static SHORT_PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:sshpass\s+-p\s*|(?:mysql|mariadb|redis-cli)\b[^\n]*?\s-[pa]\s*)")
        .expect("audit short password pattern")
});

/// Known secret-bearing suffixes are omitted before an event enters any queue.
/// This is not a general detector for arbitrary program output; output recording
/// has a separate encrypted, opt-in storage path.
pub fn redact(value: &str) -> Zeroizing<String> {
    let end = SECRET.find(value).map(|m| m.end());
    let short = SHORT_PASSWORD.find(value).map(|m| m.end());
    let cutoff = match (end, short) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let mut safe = match cutoff {
        Some(end) => Zeroizing::new(format!("{}[REDACTED]", &value[..end])),
        None => Zeroizing::new(value.to_string()),
    };
    if URI_PASSWORD.is_match(&safe) {
        safe = Zeroizing::new(
            URI_PASSWORD
                .replace_all(&safe, "${1}[REDACTED]${2}")
                .into_owned(),
        );
    }
    if TOKEN.is_match(&safe) {
        safe = Zeroizing::new(TOKEN.replace_all(&safe, "[REDACTED]").into_owned());
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_keep_useful_content_without_credentials() {
        for (input, expected) in [
            ("ls -la /srv", "ls -la /srv"),
            ("PRIVATE_KEY=short command", "PRIVATE_KEY=[REDACTED]"),
            (
                "curl --cookie tiny https://example.org",
                "curl --cookie [REDACTED]",
            ),
            ("ssh -p 2222 example.org", "ssh -p 2222 example.org"),
            (
                "curl --token 'a b c' https://example.org",
                "curl --token [REDACTED]",
            ),
            ("PASSWORD=x sudo -S cmd", "PASSWORD=[REDACTED]"),
            (
                "curl -H 'Authorization: Bearer short' /",
                "curl -H 'Authorization: [REDACTED]",
            ),
            ("sshpass -p abc ssh host", "sshpass -p [REDACTED]"),
            ("mysql -uroot -pabc", "mysql -uroot -p[REDACTED]"),
            (
                "open ssh://user:p%40ss@host:22",
                "open ssh://user:[REDACTED]@host:22",
            ),
            ("{\"password\":\"x\"}", "{\"password\":[REDACTED]"),
            (
                "-----BEGIN OPENSSH PRIVATE KEY-----\nSECRET",
                "-----BEGIN OPENSSH PRIVATE KEY-----[REDACTED]",
            ),
        ] {
            assert_eq!(redact(input).as_str(), expected, "redaction case");
        }
    }
}
