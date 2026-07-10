/// Returns the first meaningful command failure line, preferring stderr.
pub(crate) fn capture_failure_message(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    fallback: &str,
) -> String {
    let reason = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback);
    match exit_code {
        Some(code) => format!("{reason} (exit {code})"),
        None => reason.to_string(),
    }
}
