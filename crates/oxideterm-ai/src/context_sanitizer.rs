use std::sync::LazyLock;

use regex::Regex;

const REDACTED: &str = "[REDACTED]";

static PRIVATE_KEY_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"-----BEGIN\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE\s+KEY-----",
    )
    .unwrap()
});
static EXPORT_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(export\s+\w*(?:SECRET|TOKEN|PASSWORD|PASSWD|KEY|CREDENTIAL|AUTH)[A-Z_]*\s*=\s*).+",
    )
    .unwrap()
});
static KEY_VALUE_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(\w*(?:SECRET|_KEY|TOKEN|PASSWORD|PASSWD|CREDENTIAL|AUTH_TOKEN|API_KEY|APIKEY|ACCESS_KEY|PRIVATE_KEY)\s*[=:]\s*)(?:"[^"\n]{8,}"|'[^'\n]{8,}'|[^\s'";\n,)}{]{8,})"#,
    )
    .unwrap()
});
static JSON_DOUBLE_QUOTED_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)("[^"]*(?:secret|_key|token|password|passwd|credential|auth_token|api_key|apikey|access_key|private_key)"\s*:\s*")[^"\n]{8,}(")"#,
    )
    .unwrap()
});
static JSON_SINGLE_QUOTED_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)('[^']*(?:secret|_key|token|password|passwd|credential|auth_token|api_key|apikey|access_key|private_key)'\s*:\s*')[^'\n]{8,}(')"#,
    )
    .unwrap()
});
static AUTH_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b((?:Authorization|Proxy-Authorization)\s*:\s*(?:Bearer|Basic|Token|Digest)\s+)\S+",
    )
    .unwrap()
});
static AWS_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap());
static VENDOR_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-proj-[A-Za-z0-9]{20,}|sk-ant-[A-Za-z0-9]{20,}|sk_(?:live|test)_[A-Za-z0-9]{10,}|pk_(?:live|test)_[A-Za-z0-9]{10,}|rk_(?:live|test)_[A-Za-z0-9]{10,}|xox[bpoas]-[A-Za-z0-9\-]{10,})\b",
    )
    .unwrap()
});
static LONG_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Za-z0-9+/]{40,}={0,2}\b").unwrap());
static CONNECTION_PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)((?:postgres|mysql|mongodb|redis|amqp|mssql|sqlite|mariadb|cockroachdb)://[^:\s]+:)([^@\s]+)(@)")
        .unwrap()
});

pub fn sanitize_for_ai(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut result = text.to_string();
    result = PRIVATE_KEY_BLOCK
        .replace_all(
            &result,
            format!("-----BEGIN PRIVATE KEY-----\n{REDACTED}\n-----END PRIVATE KEY-----"),
        )
        .into_owned();
    result = EXPORT_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}"))
        .into_owned();
    result = KEY_VALUE_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}"))
        .into_owned();
    result = JSON_DOUBLE_QUOTED_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}${{2}}"))
        .into_owned();
    result = JSON_SINGLE_QUOTED_SECRET
        .replace_all(&result, format!("${{1}}{REDACTED}${{2}}"))
        .into_owned();
    result = AUTH_HEADER
        .replace_all(&result, format!("${{1}}{REDACTED}"))
        .into_owned();
    result = AWS_KEY.replace_all(&result, REDACTED).into_owned();
    result = VENDOR_TOKEN.replace_all(&result, REDACTED).into_owned();
    result = LONG_TOKEN
        .replace_all(&result, |captures: &regex::Captures<'_>| {
            let token = captures
                .get(0)
                .map(|value| value.as_str())
                .unwrap_or_default();
            if token.chars().any(char::is_lowercase)
                && token.chars().any(char::is_uppercase)
                && token.chars().any(|ch| ch.is_ascii_digit())
            {
                REDACTED.to_string()
            } else {
                token.to_string()
            }
        })
        .into_owned();
    CONNECTION_PASSWORD
        .replace_all(&result, format!("${{1}}{REDACTED}${{3}}"))
        .into_owned()
}
