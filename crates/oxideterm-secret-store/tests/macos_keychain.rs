// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#![cfg(target_os = "macos")]

use anyhow::{Result, ensure};
use oxideterm_secret_store::NativeSecretStore;

#[test]
#[ignore = "touches the current user's macOS keychain"]
fn real_keychain_round_trip_uses_preview_14_access() {
    let store = NativeSecretStore::new(format!("com.oxideterm.test.{}", std::process::id()));
    let account = "native-secret-store-round-trip";
    let secret = "synthetic-test-secret";

    // Cleanup runs even when the round-trip assertion reports an error.
    let result = (|| -> Result<()> {
        store.store(account, secret)?;
        ensure!(store.exists(account)?, "stored keychain entry must exist");
        ensure!(
            store.get(account)?.as_ref().map(|value| value.as_str()) == Some(secret),
            "stored keychain secret must round-trip"
        );
        Ok(())
    })();
    let cleanup = store.delete(account);

    result.expect("real keychain round-trip succeeds");
    cleanup.expect("real keychain test entry is removed");
}
