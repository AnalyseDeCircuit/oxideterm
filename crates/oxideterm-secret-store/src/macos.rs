use anyhow::{Context, Result};
use std::process::Command;
use zeroize::Zeroizing;

const SECURITY_TOOL_PATH: &str = "/usr/bin/security";
const SECURITY_ITEM_NOT_FOUND_EXIT_CODE: i32 = 44;

pub(super) fn store(service: &str, account: &str, secret: &str) -> Result<()> {
    // Preview 14 replaced the item before recreating it with an ACL that does
    // not bind access to the identity of each development rebuild.
    let _ = delete(service, account);
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
    read_password(service, account)
}

pub(super) fn delete(service: &str, account: &str) -> Result<()> {
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

fn read_password(service: &str, account: &str) -> Result<Option<Zeroizing<String>>> {
    let output = Command::new(SECURITY_TOOL_PATH)
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .context("failed to run the macOS keychain tool to load a secret")?;
    if output.status.success() {
        // The command output owns secret bytes, so zeroize that allocation after
        // moving the decoded value into its domain owner.
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

#[cfg(test)]
mod tests {
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
