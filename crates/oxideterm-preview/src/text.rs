// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub fn is_likely_text_content(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return false;
    }
    let control = sample
        .iter()
        .filter(|&&byte| matches!(byte, 0x01..=0x08 | 0x0b..=0x0c | 0x0e..=0x1f | 0x7f))
        .count();
    if control as f64 / sample.len() as f64 > 0.10 {
        return false;
    }
    std::str::from_utf8(bytes).is_ok() || sample.iter().any(|byte| *byte >= 0x80)
}

pub fn generate_hex_dump(data: &[u8], offset: u64) -> String {
    use std::fmt::Write;

    let mut result = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        let address = offset + (i * 16) as u64;
        let _ = write!(result, "{address:08X}  ");
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                result.push(' ');
            }
            let _ = write!(result, "{byte:02X} ");
        }
        for j in chunk.len()..16 {
            if j == 8 {
                result.push(' ');
            }
            result.push_str("   ");
        }
        result.push_str(" |");
        for byte in chunk {
            result.push(if (0x20..0x7f).contains(byte) {
                *byte as char
            } else {
                '.'
            });
        }
        result.push_str("|\n");
    }
    result
}

pub fn extension_to_language(ext: &str) -> Option<String> {
    let language = match ext.to_ascii_lowercase().as_str() {
        // Mirrors Tauri's SFTP `extension_to_language` table so remote
        // preview badges and syntax highlighting do not drift by frontend.
        "sh" | "bash" | "zsh" | "fish" => "bash",
        "bashrc" | "bash_profile" | "bash_login" | "bash_logout" | "bash_aliases" => "bash",
        "zshrc" | "zprofile" | "zshenv" | "zlogin" | "zlogout" => "bash",
        "profile" | "cshrc" | "tcshrc" | "kshrc" => "bash",
        "conf" | "cfg" | "ini" | "properties" => "ini",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "json" | "jsonc" | "json5" => "json",
        "xml" | "svg" | "xsd" | "xsl" => "xml",
        "html" | "htm" | "xhtml" => "html",
        "rs" => "rust",
        "py" | "pyw" | "pyi" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => "cpp",
        "java" => "java",
        "go" => "go",
        "rb" | "rake" | "gemspec" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" | "sc" => "scala",
        "r" | "rmd" => "r",
        "lua" => "lua",
        "pl" | "pm" => "perl",
        "sql" => "sql",
        "md" | "markdown" => "markdown",
        "tex" | "latex" => "latex",
        "css" | "scss" | "sass" | "less" => "css",
        "graphql" | "gql" => "graphql",
        "dockerfile" => "docker",
        "makefile" | "mk" => "makefile",
        "cmake" => "cmake",
        "nginx" => "nginx",
        "diff" | "patch" => "diff",
        "log" => "log",
        "env" | "envrc" => "bash",
        "gitignore" | "dockerignore" => "gitignore",
        "editorconfig" => "ini",
        _ => return None,
    };
    Some(language.to_string())
}

pub fn detect_and_decode(bytes: &[u8]) -> (String, String, f32, bool) {
    detect_and_decode_with_hint(bytes, None)
}

#[cfg(test)]
mod language_tests {
    use super::extension_to_language;

    #[test]
    fn sftp_preview_language_table_matches_tauri_special_cases() {
        assert_eq!(extension_to_language("zprofile").as_deref(), Some("bash"));
        assert_eq!(extension_to_language("gemspec").as_deref(), Some("ruby"));
        assert_eq!(extension_to_language("nginx").as_deref(), Some("nginx"));
        assert_eq!(
            extension_to_language("dockerignore").as_deref(),
            Some("gitignore")
        );
        assert_eq!(
            extension_to_language("editorconfig").as_deref(),
            Some("ini")
        );
    }
}

pub fn detect_and_decode_with_hint(
    bytes: &[u8],
    encoding_hint: Option<&str>,
) -> (String, String, f32, bool) {
    let (has_bom, bom_encoding) = check_bom(bytes);
    if let Some(encoding) = bom_encoding {
        let (text, _, _) = encoding.decode(bytes);
        return (text.into_owned(), encoding.name().to_string(), 1.0, true);
    }

    if let Some(encoding) = encoding_hint.and_then(|hint| {
        let hint = hint.trim();
        (!hint.is_empty())
            .then(|| encoding_rs::Encoding::for_label(hint.as_bytes()))
            .flatten()
    }) {
        let (text, _, had_errors) = encoding.decode(bytes);
        return (
            text.into_owned(),
            encoding.name().to_string(),
            if had_errors { 0.76 } else { 0.95 },
            has_bom,
        );
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let confidence = if encoding == encoding_rs::UTF_8 {
        if std::str::from_utf8(bytes).is_ok() {
            1.0
        } else {
            0.8
        }
    } else {
        0.7
    };
    let (text, _, had_errors) = encoding.decode(bytes);
    (
        text.into_owned(),
        encoding.name().to_string(),
        if had_errors {
            confidence * 0.8
        } else {
            confidence
        },
        has_bom,
    )
}

fn check_bom(bytes: &[u8]) -> (bool, Option<&'static encoding_rs::Encoding>) {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return (true, Some(encoding_rs::UTF_8));
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return (true, Some(encoding_rs::UTF_16BE));
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return (true, Some(encoding_rs::UTF_16LE));
    }
    (false, None)
}

pub fn encode_to_encoding(text: &str, encoding_name: &str) -> Vec<u8> {
    let encoding =
        encoding_rs::Encoding::for_label(encoding_name.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    if encoding == encoding_rs::UTF_8 {
        return text.as_bytes().to_vec();
    }
    let (encoded, _, _) = encoding.encode(text);
    encoded.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_hint_takes_precedence_without_bom() {
        let (encoded, _, _) = encoding_rs::GBK.encode("中文");
        let (decoded, encoding, confidence, has_bom) =
            detect_and_decode_with_hint(&encoded, Some("gbk"));

        assert_eq!(decoded, "中文");
        assert_eq!(encoding, "GBK");
        assert!(confidence > 0.9);
        assert!(!has_bom);
    }
}
