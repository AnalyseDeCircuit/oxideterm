// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Shell quoting and identifier validation for generated commands.

/// True for the Windows shell vocabulary used by the command builders.
pub fn is_windows(os_type: &str) -> bool {
    matches!(os_type, "Windows" | "windows")
}

/// Wraps a value so a POSIX shell keeps it as one literal argument.
pub fn shell_quote_single(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

/// Quotes one argv element for the shell that will run the generated line.
///
/// PowerShell does not understand the POSIX `'\''` escape, so Windows targets
/// are single-quoted with the apostrophe doubled; callers wrap the line in
/// `powershell -Command "…"` so cmd.exe and PowerShell both end up in the same
/// dialect.
///
/// That wrapper is delimited by double quotes at the cmd.exe level, where a
/// `"` inside the quoted value terminates the script regardless of PowerShell
/// quoting. A quoted value therefore cannot represent a double quote on
/// Windows: callers reject such input at the boundary (see
/// `terminal::validated_path`) instead of expecting this function to escape it.
pub fn quote_argument(os_type: &str, value: &str) -> String {
    if is_windows(os_type) {
        return format!("'{}'", value.replace('\'', "''"));
    }
    shell_quote_single(value)
}

/// Container and image identifiers are hex digests; 12 characters is the
/// shortest abbreviation every engine accepts, 64 the full digest.
pub fn validated_container_id(container_id: &str) -> Result<&str, String> {
    let container_id = container_id.trim();
    if container_id.len() < 12
        || container_id.len() > 64
        || !container_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("Invalid container identifier.".to_string());
    }
    Ok(container_id)
}

/// Volume, network and pod names allow a wider character set but never shell
/// metacharacters or whitespace. The first character must be alphanumeric so a
/// crafted name like `--all` can never be parsed as an engine CLI option.
pub fn validated_resource_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty()
        || !name.starts_with(|character: char| character.is_ascii_alphanumeric())
        || name.len() > 128
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '_' | '.' | '-' | ':' | '/' | '@')
        })
    {
        return Err("Invalid container resource reference.".to_string());
    }
    Ok(name)
}

/// Default registry for references that name none.
pub const DEFAULT_REGISTRY: &str = "docker.io";

/// True when the first path component already names a registry (`ghcr.io/x`,
/// `localhost:5000/x`). Short names like `library/nginx` do not.
fn reference_names_registry(reference: &str) -> bool {
    let Some((first, rest)) = reference.split_once('/') else {
        return false;
    };
    !rest.is_empty() && (first.contains('.') || first.contains(':') || first == "localhost")
}

/// Normalizes a registry field: trims surrounding space and slashes and
/// accepts a pasted scheme URL (`https://ghcr.io/`).
pub fn normalized_registry(registry: &str) -> String {
    let registry = registry.trim();
    let registry = registry
        .strip_prefix("https://")
        .or_else(|| registry.strip_prefix("http://"))
        .unwrap_or(registry);
    registry.trim_matches('/').to_string()
}

/// Combines the registry field with an image reference. An empty registry
/// means Docker Hub, and a reference that already names a registry is left
/// untouched so pasting a full name never doubles the host.
pub fn qualified_image_reference(registry: &str, reference: &str) -> String {
    let reference = reference.trim();
    if reference.is_empty() || reference_names_registry(reference) {
        return reference.to_string();
    }
    let registry = normalized_registry(registry);
    let registry = if registry.is_empty() {
        DEFAULT_REGISTRY
    } else {
        &registry
    };
    format!("{registry}/{reference}")
}

/// Versions come from remote output; keep them printable and bounded before
/// they reach a list row.
pub fn sanitized_version(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let sanitized: String = raw
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '+' | '-' | '_')
        })
        .take(32)
        .collect();
    (!sanitized.is_empty()).then_some(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_references_are_qualified_with_the_selected_registry() {
        // Empty registry means Docker Hub.
        assert_eq!(
            qualified_image_reference("", "nginx:alpine"),
            "docker.io/nginx:alpine"
        );
        assert_eq!(
            qualified_image_reference("quay.io/", "prometheus/prometheus"),
            "quay.io/prometheus/prometheus"
        );
        assert_eq!(
            qualified_image_reference("https://ghcr.io", "me/app:1"),
            "ghcr.io/me/app:1"
        );
        // A reference that already names its registry keeps it, so pasting a
        // full image name never doubles the host.
        for reference in ["ghcr.io/me/app:1", "localhost:5000/app", "localhost/app"] {
            assert_eq!(qualified_image_reference("docker.io", reference), reference);
        }
        // Namespace-only paths (no dot/colon in the first segment) are not hosts.
        assert_eq!(
            qualified_image_reference("", "library/nginx:alpine"),
            "docker.io/library/nginx:alpine"
        );
    }

    #[test]
    fn quoting_and_validation_reject_shell_metacharacters() {
        assert_eq!(shell_quote_single("a'b"), "'a'\\''b'");
        assert!(validated_container_id("a1b2c3d4e5f6").is_ok());
        assert!(validated_container_id("abc123").is_err());
        assert!(validated_container_id("a1b2c3d4e5f6; rm -rf /").is_err());
        assert!(validated_resource_name("app-data").is_ok());
        assert!(validated_resource_name("my-pod").is_ok());
        assert!(validated_resource_name("a1b2").is_ok());
        // A leading dash would be parsed as an engine option, not a name.
        assert!(validated_resource_name("--all").is_err());
        assert!(validated_resource_name("-v").is_err());
        assert!(validated_resource_name("bad name").is_err());
        assert_eq!(sanitized_version(" 5.8.2\n"), Some("5.8.2".to_string()));
        assert_eq!(
            sanitized_version("3.0.3.0\r\n"),
            Some("3.0.3.0".to_string())
        );
        assert_eq!(
            sanitized_version("5.8.2\u{0}\u{1b}"),
            Some("5.8.2".to_string())
        );
    }
}
