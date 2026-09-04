use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use keyring::{Entry, Error as KeyringError};
use std::process::Command;
use zeroize::Zeroizing;

const SECURITY_TOOL_PATH: &str = "/usr/bin/security";
const SECURITY_ITEM_NOT_FOUND_EXIT_CODE: i32 = 44;
const VERSIONED_ACCOUNT_PREFIX: &str = "oxideterm-secret-v1:";

pub(super) fn store(service: &str, account: &str, secret: &str) -> Result<()> {
    let versioned_account = versioned_account(account);
    let encoded_secret = Zeroizing::new(STANDARD_NO_PAD.encode(secret.as_bytes()));
    store_password_with_security_tool(service, &versioned_account, &encoded_secret)?;

    // Write the new value before deleting its predecessor so a failed write cannot lose the
    // only stored copy. Reads prefer the versioned account if legacy cleanup is interrupted.
    delete_password_with_security_tool(service, account)
}

fn store_password_with_security_tool(service: &str, account: &str, secret: &str) -> Result<()> {
    // Preview 14 replaced the item before recreating it with an ACL that does
    // not bind access to the identity of each development rebuild.
    let _ = delete_password_with_security_tool(service, account);
    let output = Command::new(SECURITY_TOOL_PATH)
        .args([
            "add-generic-password",
            "-s",
            service,
            "-a",
            account,
            "-w",
            secret,
            "-A",
        ])
        .output()
        .context("failed to run the macOS keychain tool to store a secret")?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!("failed to store a secret in the macOS keychain")
    }
}

pub(super) fn get(service: &str, account: &str) -> Result<Option<Zeroizing<String>>> {
    if let Some(secret) = read_versioned_password(service, account)? {
        return Ok(Some(secret));
    }
    read_password_with_security_tool(service, account)
}

pub(super) fn get_preserving_text(
    service: &str,
    account: &str,
) -> Result<Option<Zeroizing<String>>> {
    if let Some(secret) = read_versioned_password(service, account)? {
        return Ok(Some(secret));
    }

    let Some(cli_secret) = read_password_with_security_tool(service, account)? else {
        return Ok(None);
    };
    if !could_be_cli_hex_rendering(&cli_secret) {
        return Ok(Some(cli_secret));
    }

    // A legacy password consisting only of hexadecimal digits is indistinguishable from the
    // CLI's hexadecimal rendering. Read the source value natively once, then migrate it so both
    // cases preserve their exact text without prompting again on later reads.
    let native_secret = read_password_with_native_keychain(service, account)?.ok_or_else(|| {
        anyhow::anyhow!("macOS keychain entry disappeared during legacy secret migration")
    })?;
    migrate_legacy_password(service, account, &native_secret)?;
    Ok(Some(native_secret))
}

pub(super) fn get_preserving_multiline(
    service: &str,
    account: &str,
) -> Result<Option<Zeroizing<String>>> {
    if let Some(secret) = read_versioned_password(service, account)? {
        return Ok(Some(secret));
    }

    let Some(secret) = read_password_with_native_keychain(service, account)? else {
        return Ok(None);
    };
    migrate_legacy_password(service, account, &secret)?;
    Ok(Some(secret))
}

pub(super) fn delete(service: &str, account: &str) -> Result<()> {
    let versioned_result = delete_password_with_security_tool(service, &versioned_account(account));
    let legacy_result = delete_password_with_security_tool(service, account);
    versioned_result.and(legacy_result)
}

fn delete_password_with_security_tool(service: &str, account: &str) -> Result<()> {
    let output = Command::new(SECURITY_TOOL_PATH)
        .args(["delete-generic-password", "-s", service, "-a", account])
        .output()
        .context("failed to run the macOS keychain tool to delete a secret")?;
    if output.status.success() || output.status.code() == Some(SECURITY_ITEM_NOT_FOUND_EXIT_CODE) {
        Ok(())
    } else {
        anyhow::bail!("failed to delete a secret from the macOS keychain")
    }
}

pub(super) fn exists(service: &str, account: &str) -> Result<bool> {
    if password_exists_with_security_tool(service, &versioned_account(account))? {
        return Ok(true);
    }
    password_exists_with_security_tool(service, account)
}

fn password_exists_with_security_tool(service: &str, account: &str) -> Result<bool> {
    let output = Command::new(SECURITY_TOOL_PATH)
        .args(["find-generic-password", "-s", service, "-a", account])
        .output()
        .context("failed to run the macOS keychain tool to inspect a secret")?;
    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(SECURITY_ITEM_NOT_FOUND_EXIT_CODE) {
        return Ok(false);
    }
    anyhow::bail!("failed to inspect a macOS keychain entry")
}

fn read_versioned_password(service: &str, account: &str) -> Result<Option<Zeroizing<String>>> {
    let Some(encoded_secret) =
        read_password_with_security_tool(service, &versioned_account(account))?
    else {
        return Ok(None);
    };
    decode_versioned_secret(&encoded_secret).map(Some)
}

fn decode_versioned_secret(encoded_secret: &str) -> Result<Zeroizing<String>> {
    let decoded_bytes = Zeroizing::new(
        STANDARD_NO_PAD
            .decode(encoded_secret.as_bytes())
            .context("versioned macOS keychain secret is not valid Base64")?,
    );
    let secret = std::str::from_utf8(decoded_bytes.as_slice())
        .context("versioned macOS keychain secret is not valid UTF-8")?;
    Ok(Zeroizing::new(secret.to_owned()))
}

fn migrate_legacy_password(service: &str, account: &str, secret: &str) -> Result<()> {
    store(service, account, secret).context("failed to migrate a legacy macOS keychain secret")
}

fn versioned_account(account: &str) -> String {
    format!("{VERSIONED_ACCOUNT_PREFIX}{account}")
}

fn could_be_cli_hex_rendering(secret: &str) -> bool {
    let encoded = secret.strip_prefix("0x").unwrap_or(secret).as_bytes();
    !encoded.is_empty()
        && encoded.len().is_multiple_of(2)
        && encoded.iter().all(u8::is_ascii_hexdigit)
}

fn read_password_with_security_tool(
    service: &str,
    account: &str,
) -> Result<Option<Zeroizing<String>>> {
    let output = Command::new(SECURITY_TOOL_PATH)
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .context("failed to run the macOS keychain tool to load a secret")?;
    if output.status.success() {
        // The subprocess buffer owns secret bytes and is scrubbed after the
        // decoded value moves into the caller's zeroizing allocation.
        let output = Zeroizing::new(output.stdout);
        let secret = std::str::from_utf8(output.as_slice())
            .context("macOS keychain secret is not valid UTF-8")?;
        return Ok(Some(Zeroizing::new(
            secret.trim_end_matches(['\r', '\n']).to_owned(),
        )));
    }
    if output.status.code() == Some(SECURITY_ITEM_NOT_FOUND_EXIT_CODE) {
        Ok(None)
    } else {
        anyhow::bail!("failed to load a secret from the macOS keychain")
    }
}

fn read_password_with_native_keychain(
    service: &str,
    account: &str,
) -> Result<Option<Zeroizing<String>>> {
    // Native reads are limited to legacy values whose exact representation cannot be recovered
    // from the CLI. Versioned values stay on the prompt-free CLI path.
    let entry = Entry::new(service, account)
        .context("failed to open a macOS keychain entry for secret loading")?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(Zeroizing::new(secret))),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(error).context("failed to load a secret from the macOS keychain"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        STANDARD_NO_PAD, could_be_cli_hex_rendering, decode_versioned_secret, versioned_account,
    };
    use base64::Engine as _;

    #[test]
    fn legacy_hex_detection_covers_ambiguous_passwords_and_cli_prefixes() {
        assert!(could_be_cli_hex_rendering("deadbeef0123"));
        assert!(could_be_cli_hex_rendering("0xE5AF86E7A081"));
        assert!(!could_be_cli_hex_rendering("ssh-password"));
        assert!(!could_be_cli_hex_rendering("abc"));
    }

    #[test]
    fn versioned_payload_round_trips_unicode_and_multiline_text() {
        let account = versioned_account("connection-id");
        let secret = "line one\n密码";
        let encoded = STANDARD_NO_PAD.encode(secret.as_bytes());

        assert_eq!(account, "oxideterm-secret-v1:connection-id");
        assert!(encoded.is_ascii());
        assert!(!encoded.contains('\n'));
        assert_eq!(decode_versioned_secret(&encoded).unwrap().as_str(), secret);
    }

    #[test]
    fn preview_14_store_arguments_include_permissive_acl() {
        let source = include_str!("macos.rs");

        assert!(source.contains("\"add-generic-password\""));
        assert!(source.contains("\"-A\""));
    }

    #[test]
    fn existence_lookup_does_not_request_secret_data() {
        let source = include_str!("macos.rs");
        let exists_source = source
            .split("pub(super) fn exists")
            .nth(1)
            .and_then(|source| source.split("fn read_password").next())
            .expect("exists function precedes read_password");

        assert!(!exists_source.contains("\"-w\""));
    }
}
