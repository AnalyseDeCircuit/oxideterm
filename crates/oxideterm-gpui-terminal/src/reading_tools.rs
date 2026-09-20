use chrono::{DateTime, Local, Utc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtractionShell {
    Posix,
    Fish,
    PowerShell,
}

pub(crate) enum NumberInspection {
    Timestamp { utc: String, local: String },
    Hex { decimal: String, binary: String },
}

pub(crate) fn inspect_number(text: &str) -> Option<NumberInspection> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        if hex.is_empty() || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let number = u64::from_str_radix(hex, 16).ok()?;
        return Some(NumberInspection::Hex {
            decimal: number.to_string(),
            binary: format!("0b{number:b}"),
        });
    }
    if !matches!(text.len(), 10 | 13) || !text.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    let time = if text.len() == 10 {
        DateTime::<Utc>::from_timestamp(value, 0)?
    } else {
        DateTime::<Utc>::from_timestamp_millis(value)?
    };
    Some(NumberInspection::Timestamp {
        utc: time.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
        local: format_local_time(time.with_timezone(&Local)),
    })
}

fn format_local_time<T: chrono::TimeZone>(time: DateTime<T>) -> String
where
    T::Offset: std::fmt::Display,
{
    time.format("%Y-%m-%d %H:%M:%S%.3f %:z").to_string()
}

pub fn extraction_command(path: &str, shell: ExtractionShell) -> Option<String> {
    if path.is_empty() || path.contains(['\0', '\r', '\n']) {
        return None;
    }
    let lower = path.to_ascii_lowercase();
    let program = if [
        ".tar",
        ".tar.gz",
        ".tgz",
        ".tar.bz2",
        ".tbz2",
        ".tar.xz",
        ".txz",
        ".tbz",
        ".tar.zst",
        ".tzst",
        ".tar.z",
        ".taz",
        ".tar.lz",
        ".tar.lzma",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
    {
        "tar -xf"
    } else if lower.ends_with(".zip") {
        if shell == ExtractionShell::PowerShell {
            "Expand-Archive -LiteralPath"
        } else {
            "unzip"
        }
    } else if lower.ends_with(".gz") {
        "gzip -dk"
    } else if lower.ends_with(".bz2") {
        "bzip2 -dk"
    } else if lower.ends_with(".xz") {
        "xz -dk"
    } else if lower.ends_with(".7z") {
        "7z x"
    } else if lower.ends_with(".rar") {
        "unrar x"
    } else {
        return None;
    };
    // Prefix relative option-looking names; quoting alone does not stop option parsing.
    let path = if path.starts_with(['-', '@']) {
        format!("./{path}")
    } else {
        path.to_string()
    };
    let quoted = match shell {
        ExtractionShell::Posix => format!("'{}'", path.replace('\'', "'\\''")),
        ExtractionShell::Fish => format!("'{}'", path.replace('\\', "\\\\").replace('\'', "\\'")),
        ExtractionShell::PowerShell => format!("'{}'", path.replace('\'', "''")),
    };
    Some(format!("{program} {quoted}"))
}

pub(crate) fn remove_line_numbers(text: &str) -> Option<String> {
    let mut expected = None;
    let mut separator = None;
    let mut output = String::new();
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            output.push_str(line);
            continue;
        }
        let count = line.bytes().take_while(u8::is_ascii_digit).count();
        if count == 0 {
            return None;
        }
        let number: u64 = line[..count].parse().ok()?;
        let rest = &line[count..];
        let (delimiter, rest) = if let Some(rest) = rest.strip_prefix(". ") {
            (". ", rest)
        } else if let Some(rest) = rest.strip_prefix(": ") {
            (": ", rest)
        } else if let Some(rest) = rest.strip_prefix('\t') {
            ("\t", rest)
        } else if let Some(rest) = rest.strip_prefix(' ') {
            (" ", rest)
        } else {
            return None;
        };
        if expected.is_some_and(|expected| number != expected)
            || separator.is_some_and(|previous| previous != delimiter)
        {
            return None;
        }
        expected = number.checked_add(1);
        if expected.is_none() {
            return None;
        }
        separator = Some(delimiter);
        output.push_str(rest);
    }
    separator.map(|_| output)
}

pub(crate) fn remove_prefix(text: &str, prefix: &str) -> Option<String> {
    if prefix.is_empty() || prefix.contains(['\r', '\n']) {
        return None;
    }
    let mut output = String::new();
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            output.push_str(line);
        } else {
            output.push_str(line.strip_prefix(prefix)?);
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamps_and_hex_use_complete_tokens_and_reject_overflow() {
        let time = DateTime::<Utc>::from_timestamp_millis(1700000000123).unwrap();
        assert_eq!(
            format_local_time(
                time.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap())
            ),
            "2023-11-15 06:13:20.123 +08:00"
        );
        assert_eq!(
            format_local_time(
                time.with_timezone(&chrono::FixedOffset::west_opt(5 * 3600).unwrap())
            ),
            "2023-11-14 17:13:20.123 -05:00"
        );
        let Some(NumberInspection::Hex { decimal, .. }) = inspect_number("0xFFFFFFFFFFFFFFFF")
        else {
            panic!("u64 maximum")
        };
        assert_eq!(decimal, "18446744073709551615");
        for input in ["1700000000", "1700000000000"] {
            let Some(NumberInspection::Timestamp { utc, .. }) = inspect_number(input) else {
                panic!("timestamp expected")
            };
            assert_eq!(utc, "2023-11-14 22:13:20.000 UTC");
        }
        let Some(NumberInspection::Timestamp { utc, .. }) = inspect_number("1234567890123") else {
            panic!("millisecond timestamp expected")
        };
        assert_eq!(utc, "2009-02-13 23:31:30.123 UTC");
        let Some(NumberInspection::Hex { decimal, binary }) = inspect_number("0xFF") else {
            panic!("hex expected")
        };
        assert_eq!((decimal.as_str(), binary.as_str()), ("255", "0b11111111"));
        for input in [
            "x1700000000",
            "17000000000",
            "0x",
            "0x10000000000000000",
            "0xffx",
        ] {
            assert!(inspect_number(input).is_none(), "{input}");
        }
    }
    #[test]
    fn archive_commands_quote_paths_for_the_selected_shell() {
        for (path, shell, expected) in [
            (
                "my file.tar.gz",
                ExtractionShell::Posix,
                "tar -xf 'my file.tar.gz'",
            ),
            (
                "a';$(touch x).zip",
                ExtractionShell::Posix,
                "unzip 'a'\\'';$(touch x).zip'",
            ),
            ("-file.zip", ExtractionShell::Posix, "unzip './-file.zip'"),
            (
                "a'b.zip",
                ExtractionShell::PowerShell,
                "Expand-Archive -LiteralPath 'a''b.zip'",
            ),
            ("a'b.7z", ExtractionShell::Fish, "7z x 'a\\'b.7z'"),
            ("@names.7z", ExtractionShell::Posix, "7z x './@names.7z'"),
            (
                "data.tar.zst",
                ExtractionShell::Posix,
                "tar -xf 'data.tar.zst'",
            ),
            ("data.bz2", ExtractionShell::Posix, "bzip2 -dk 'data.bz2'"),
            ("data.gz", ExtractionShell::Posix, "gzip -dk 'data.gz'"),
            ("data.xz", ExtractionShell::Posix, "xz -dk 'data.xz'"),
            ("data.rar", ExtractionShell::Posix, "unrar x 'data.rar'"),
        ] {
            assert_eq!(extraction_command(path, shell).as_deref(), Some(expected));
        }
        for path in ["file.txt", "x\nfile.zip", ""] {
            assert_eq!(extraction_command(path, ExtractionShell::Posix), None);
        }
    }
    #[test]
    fn paste_transforms_preserve_indentation_and_reject_ambiguous_input() {
        assert_eq!(
            remove_line_numbers("7. echo ok\r\n8.   echo done\r\n").as_deref(),
            Some("echo ok\r\n  echo done\r\n")
        );
        assert_eq!(
            remove_prefix("$ echo ok\n$   echo done\n", "$ ").as_deref(),
            Some("echo ok\n  echo done\n")
        );
        for input in ["1. a\n3. b", "1. a\n2: b", "a\n2. b"] {
            assert_eq!(remove_line_numbers(input), None);
        }
        assert_eq!(remove_prefix("$ a\nactual output", "$ "), None);
    }
}
