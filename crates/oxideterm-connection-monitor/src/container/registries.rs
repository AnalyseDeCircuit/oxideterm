// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Authenticated registries from the engine's auth file.
//!
//! Docker stores credentials in `config.json` (`auths` map) and Podman in
//! `auth.json`, which uses the same shape. Only registry keys leave the host:
//! the values hold base64 `user:password`, identity tokens and helper names, and
//! none of them are ever read into a row, a log or a notice.

use serde::{Deserialize, Serialize};

use crate::container::engine::ContainerEngine;
use crate::container::instance::ContainerInstance;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRegistrySource {
    Docker,
    Podman,
}

impl ContainerRegistrySource {
    pub fn id(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Docker => "sidebar.host_container.sources.docker",
            Self::Podman => "sidebar.host_container.sources.podman",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "docker" => Some(Self::Docker),
            "podman" => Some(Self::Podman),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerRegistry {
    pub source: ContainerRegistrySource,
    pub registry: String,
}

/// Extracts the keys of the `auths` object without leaving the host.
///
/// The lexer streams the file one character at a time: no copy of the file and
/// no credential value survives in a variable (strings are capped at 256
/// bytes, which is also the point where a string stops being a plausible
/// registry key). Nothing is printed unless the whole document parses, so a
/// truncated auth file never yields a registry row.
const AUTH_KEYS_AWK: &str = r#"awk '
BEGIN { depth = 0; target = -1; in_str = 0; esc = 0; await_colon = 0; oversize = 0; stored = 0; token = ""; pending = ""; last_key = ""; count = 0; invalid = 0 }
{
  n = length($0)
  for (i = 1; i <= n; i++) {
    c = substr($0, i, 1)
    if (in_str) {
      if (esc) {
        if (stored < 256) token = token "\\" c; else oversize = 1
        esc = 0; stored++
      } else if (c == "\\") {
        esc = 1
      } else if (c == "\"") {
        in_str = 0; pending = token; await_colon = 1
      } else {
        if (stored < 256) token = token c; else oversize = 1
        stored++
      }
      continue
    }
    if (await_colon) {
      if (c == " " || c == "\t" || c == "\r") continue
      await_colon = 0
      if (c == ":") {
        # Escape-sensitive keys stay verbatim so the reader can decode them.
        if (!oversize) { last_key = pending; if (target > 0 && depth == target) keys[++count] = pending }
        continue
      }
    }
    if (c == "\"") {
      in_str = 1; token = ""; stored = 0; oversize = 0
    } else if (c == "{") {
      depth++
      if (last_key == "auths" && target < 0) target = depth
    } else if (c == "}") {
      if (depth == target) { target = -1; last_key = "" }
      depth--
      if (depth < 0) { invalid = 1; exit }
    }
  }
}
END {
  if (invalid || depth != 0 || in_str || esc || await_colon || target >= 0) exit 0
  for (k = 1; k <= count; k++) print keys[k]
}'"#;

pub fn registries_command(os_type: &str, instance: &ContainerInstance) -> String {
    match os_type {
        "Windows" | "windows" => crate::container::windows::posix_via_bash(
            &posix_registries(instance),
            "CONTAINER_REGISTRIES",
            "Git Bash (bash.exe) not found",
        ),
        // The embedded AWK lexer uses literal double-backslashes and newlines,
        // so encode the script to keep Fish from rewriting `\\` inside quotes.
        _ => crate::shell::encoded_posix_shell_command(&posix_registries(instance)),
    }
}

fn posix_files(engine: ContainerEngine) -> &'static str {
    match engine {
        // Podman: explicit override, runtime socket dir, user config, then the
        // Docker file podman falls back to when its own auth file is missing.
        ContainerEngine::Podman => {
            r#""$REGISTRY_AUTH_FILE" "${XDG_RUNTIME_DIR}/containers/auth.json" "$HOME/.config/containers/auth.json" "${APPDATA}/containers/auth.json" "$HOME/.docker/config.json""#
        }
        // Docker: DOCKER_CONFIG replaces the default home config entirely, so
        // the home file is only consulted when it is unset or empty.
        _ => r#""${DOCKER_CONFIG:-$HOME/.docker}/config.json""#,
    }
}

fn posix_registries(instance: &ContainerInstance) -> String {
    let source = match instance.engine {
        ContainerEngine::Podman => "podman",
        _ => "docker",
    };
    // Only the file the engine would actually read is listed: Podman stops at
    // `REGISTRY_AUTH_FILE` and otherwise falls back in order, so reading every
    // candidate would report registries the CLI will not use.
    format!(
        "echo '===CONTAINER_REGISTRIES==='; \
         for oxide_file in {}; do \
         [ -n \"$oxide_file\" ] && [ -f \"$oxide_file\" ] || continue; \
         {AUTH_KEYS_AWK} \"$oxide_file\" 2>/dev/null | head -n 100 | awk -v src='{source}' '{{print \"REG\\t\" src \"\\t\" $0}}'; \
         break; \
         done; \
         echo '===CONTAINER_REGISTRIES_END==='",
        posix_files(instance.engine)
    )
}

pub fn parse_registries(output: &str) -> Vec<ContainerRegistry> {
    let Some(section) = extract_section(output, "CONTAINER_REGISTRIES") else {
        return Vec::new();
    };
    let mut registries: Vec<ContainerRegistry> = Vec::new();
    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut columns = line.split('\t');
        if columns.next() != Some("REG") {
            continue;
        }
        let Some(source) = columns.next().and_then(ContainerRegistrySource::parse) else {
            continue;
        };
        let Some(raw) = columns.next() else {
            continue;
        };
        let Some(registry) = normalize_registry(raw) else {
            continue;
        };
        if registries
            .iter()
            .any(|existing| existing.registry.eq_ignore_ascii_case(&registry))
        {
            continue;
        }
        registries.push(ContainerRegistry { source, registry });
    }
    registries.sort_by(|left, right| left.registry.cmp(&right.registry));
    registries
}

/// Registry keys are URLs or hosts; show the host (and any path) without the
/// scheme or trailing slash, and reject control characters from crafted files.
///
/// The lexer hands JSON escapes over verbatim, so they are decoded first: a
/// key written as `"badt\u0009x"` must be rejected, not shown as `badtx`.
fn normalize_registry(raw: &str) -> Option<String> {
    let decoded = decode_json_escapes(raw)?;
    let mut value = decoded.trim();
    if let Some(rest) = value.strip_prefix("https://") {
        value = rest;
    }
    if let Some(rest) = value.strip_prefix("http://") {
        value = rest;
    }
    // A query or fragment is not part of a registry identity, and neither are
    // the credentials some tools embed in the key (`user:token@host`).
    let value = value
        .split(|character| character == '?' || character == '#')
        .next()
        .unwrap_or("");
    let (authority, path) = match value.split_once('/') {
        Some((authority, path)) => (authority, Some(path)),
        None => (value, None),
    };
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let mut value = authority.to_string();
    if let Some(path) = path {
        value.push('/');
        value.push_str(path);
    }
    let value = value.trim_end_matches('/');
    if value.is_empty()
        || value.len() > 200
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return None;
    }
    Some(value.to_string())
}

/// Decodes the JSON string escapes that `AUTH_KEYS_AWK` passes through. An
/// unsupported escape means the key is malformed, so it is rejected instead of
/// being shown as a different host.
fn decode_json_escapes(raw: &str) -> Option<String> {
    let mut decoded = String::with_capacity(raw.len());
    let mut characters = raw.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next()? {
            '"' => decoded.push('"'),
            '\\' => decoded.push('\\'),
            '/' => decoded.push('/'),
            'b' => decoded.push('\u{8}'),
            'f' => decoded.push('\u{c}'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            'u' => {
                let mut code = 0u32;
                for _ in 0..4 {
                    code = code * 16 + characters.next()?.to_digit(16)?;
                }
                // Registry hosts never contain surrogate pairs; rejecting them
                // beats emitting half a code point.
                if (0xd800..=0xdfff).contains(&code) {
                    return None;
                }
                decoded.push(char::from_u32(code)?);
            }
            _ => return None,
        }
    }
    Some(decoded)
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(match after_start.split_once(&end) {
        Some((section, _)) => section.trim(),
        None => after_start.trim(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::transport::ContainerTransport;

    fn instance(engine: ContainerEngine) -> ContainerInstance {
        ContainerInstance::new(engine, ContainerTransport::Cli, None)
    }

    #[test]
    fn parses_and_dedupes_registries_across_files() {
        let output = concat!(
            "===CONTAINER_REGISTRIES===\n",
            "REG\tpodman\t423437244826.dkr.ecr.ap-southeast-1.amazonaws.com\n",
            "REG\tpodman\tharbor.emea.ocp.int.kn\n",
            "REG\tpodman\thttps://index.docker.io/v1/\n",
            "REG\tpodman\tharbor.emea.ocp.int.kn\n",
            "REG\tdocker\tghcr.io\n",
            "bogus line\n",
            "===CONTAINER_REGISTRIES_END===",
        );
        let registries = parse_registries(output);
        assert_eq!(registries.len(), 4);
        assert_eq!(
            registries[0].registry,
            "423437244826.dkr.ecr.ap-southeast-1.amazonaws.com"
        );
        assert_eq!(registries[0].source, ContainerRegistrySource::Podman);
        assert!(
            registries
                .iter()
                .any(|registry| registry.registry == "index.docker.io/v1")
        );
        assert_eq!(
            registries
                .iter()
                .find(|registry| registry.registry == "ghcr.io")
                .map(|registry| registry.source),
            Some(ContainerRegistrySource::Docker)
        );
    }

    #[test]
    fn missing_section_and_bad_keys_are_ignored() {
        assert!(parse_registries("").is_empty());
        let output = "===CONTAINER_REGISTRIES===\nREG\tpodman\thas space\nREG\tunknown\tx\n===CONTAINER_REGISTRIES_END===";
        assert!(parse_registries(output).is_empty());
    }

    #[test]
    fn credentials_in_a_key_never_reach_the_registry_row() {
        // Keys written as URLs may carry userinfo; the auth value already holds
        // the secret, and a token must not be rendered by the panel.
        let output = concat!(
            "===CONTAINER_REGISTRIES===\n",
            "REG\tpodman\thttps://alice:secret@registry.example/\n",
            "REG\tpodman\tregistry.example/v2?ns=1#tag\n",
            "REG\tdocker\tghcr.io\n",
            "REG\tdocker\tindex.docker.io/v1/\n",
            "===CONTAINER_REGISTRIES_END===",
        );
        let registries = parse_registries(output);
        let hosts = registries
            .iter()
            .map(|registry| registry.registry.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            hosts,
            [
                "ghcr.io",
                "index.docker.io/v1",
                "registry.example",
                "registry.example/v2"
            ]
        );
        assert!(
            registries
                .iter()
                .all(|registry| !registry.registry.contains('@')
                    && !registry.registry.contains("alice")
                    && !registry.registry.contains('?'))
        );
    }

    #[test]
    fn json_escapes_are_decoded_before_a_key_is_accepted() {
        // The awk lexer preserves the escape; the reader must decode it so a
        // newline/tab key is rejected instead of becoming `badtx`.
        let output = concat!(
            "===CONTAINER_REGISTRIES===\n",
            "REG\tpodman\tbadt\\name\n",
            "REG\tpodman\tbad\\tx\n",
            "REG\tdocker\treg\\u0069stry.example\n",
            "REG\tdocker\tunsupported\\qescape\n",
            "REG\tdocker\tunterminated\\\n",
            "REG\tdocker\tghcr.io\n",
            "===CONTAINER_REGISTRIES_END===",
        );
        let registries = parse_registries(output);
        assert_eq!(
            registries
                .iter()
                .map(|registry| registry.registry.as_str())
                .collect::<Vec<_>>(),
            ["ghcr.io", "registry.example"]
        );
    }

    #[test]
    fn auth_key_lexer_preserves_json_escapes_for_the_reader() {
        // Dropping the backslash here would hand the reader a valid-looking but
        // different host, so the raw escape must survive to `decode_json_escapes`.
        let command = registries_command("Linux", &instance(ContainerEngine::Docker));
        assert!(command.starts_with("/bin/sh -c "));
        let decoded = crate::shell::decoded_posix_script(&command);
        assert!(decoded.contains("token = token \"\\\\\" c"));
    }

    #[test]
    fn commands_read_only_the_engine_auth_files() {
        let podman = crate::shell::decoded_posix_script(&registries_command(
            "Linux",
            &instance(ContainerEngine::Podman),
        ));
        assert!(podman.contains("$REGISTRY_AUTH_FILE"));
        assert!(podman.contains("$HOME/.config/containers/auth.json"));
        assert!(podman.contains("$HOME/.docker/config.json"));
        assert!(!podman.contains("auth\":"));
        let docker = crate::shell::decoded_posix_script(&registries_command(
            "Linux",
            &instance(ContainerEngine::Docker),
        ));
        // DOCKER_CONFIG replaces the home config entirely instead of being a
        // first candidate.
        assert!(docker.contains("\"${DOCKER_CONFIG:-$HOME/.docker}/config.json\""));
        assert!(!docker.contains("containers/auth.json"));
        let windows = registries_command("Windows", &instance(ContainerEngine::Podman));
        assert!(windows.contains("FromBase64String("));
        assert!(windows.contains("Git Bash"));
    }
}
