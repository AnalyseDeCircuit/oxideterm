use std::borrow::Cow;

/// Only embedded program sources pass this boundary, before user paths or
/// configuration are interpolated. Existing checkouts may still contain CRLF
/// even after the repository starts enforcing LF for fresh checkouts.
pub(crate) fn normalize_integration_script(source: &'static str) -> Cow<'static, str> {
    if source.contains("\r\n") {
        Cow::Owned(source.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_only_crlf_preserving_script_bytes_and_trailing_lines() {
        for (source, expected) in [
            (
                "hook() {\r\n  printf '中文\\r\\n'\n}\r\n\r\n",
                "hook() {\n  printf '中文\\r\\n'\n}\n\n",
            ),
            (
                "printf 'literal\rvalue'\r\nlast",
                "printf 'literal\rvalue'\nlast",
            ),
            ("#!/bin/sh\necho done\n", "#!/bin/sh\necho done\n"),
        ] {
            assert_eq!(normalize_integration_script(source), expected);
        }
    }
}
