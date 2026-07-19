---
name: oxideterm-secret-zeroize
description: Apply OxideTerm's secret lifetime and redaction rules when code handles passwords, passphrases, private keys, tokens, authentication headers, proxy credentials, prompts, diagnostics, persistence, or async task capture.
---

# OxideTerm Secret Zeroize

Use this skill before editing any path that receives, owns, transforms, persists, logs, reports, or asynchronously captures credential-like values.

## Core invariant

Every secret must have an explicit owner, output boundary, and zeroization point. A value is not safe merely because it remains in memory or is omitted from the normal UI.

## Secret classification

Treat at least these values as secrets:

- SSH, sudo, proxy, SFTP, and cloud-sync passwords.
- Private-key passphrases and decrypted private-key bytes.
- Keyboard-interactive answers and authentication prompt drafts.
- API tokens, bearer values, cookies, and authorization headers.
- Jump-host and upstream-proxy credentials.
- Serialized authentication material before encryption.
- Commands, environment variables, config fragments, terminal text, or diagnostics that may embed credentials.

## Mandatory rules

- Prefer `zeroize::Zeroizing<T>` for owned temporary values.
- Types that own secret buffers must use `Zeroize`, `ZeroizeOnDrop`, or an equivalent zeroizing wrapper.
- UI controls may temporarily require `String`, but the UI/backend handoff must convert the value into a zeroizing type and clearly define which object clears the UI draft.
- Do not clone secrets for convenience. Every unavoidable copy needs the same lifetime and zeroization treatment.
- Do not derive `Debug` for a type that can expose a secret. Implement a redacted formatter that preserves only safe structural information.
- Never include secret values in tracing, logs, errors, panic messages, notifications, connection keys, task names, or diagnostic bundles.
- Never send raw secret-bearing content to AI prompts, agents, tool calls, telemetry, issue reports, or support bundles. Redact before constructing the outgoing payload.
- Persist secrets only through the repository's designated encrypted or OS-protected secret store. Plain settings and connection metadata may contain references or non-secret capability markers, not the value.
- Spawned tasks and process bridges must not retain unbounded secret clones. Move the smallest zeroizing value into the task and tie its lifetime to an explicit runtime owner.
- Add concise English comments at non-obvious secret boundaries explaining ownership, redaction, persistence, or drop behavior.

## Recommended practices

- Prefer byte buffers when the downstream cryptographic or protocol API accepts them.
- Parse or transform secrets in place when practical.
- Keep redacted metadata separate from secret-bearing runtime types.
- Use one-way digests when equality or cache identity is required without retaining the original value.
- Keep error messages actionable by naming the failed operation, not the secret input.
- Clear obsolete UI drafts immediately after successful handoff, cancellation, or replacement.

## Compatibility paths

Compatibility with an older frontend or persistence format is not authority to introduce new plaintext storage. Existing legacy plaintext must be migrated into the current protected representation and temporary plaintext buffers must be zeroized. Document any unavoidable compatibility path and its removal boundary.

## Examples

Incorrect:

```rust
#[derive(Debug, Clone)]
struct LoginAttempt {
    password: String,
}

tracing::warn!(?attempt, "authentication failed");
```

Correct shape:

```rust
struct LoginAttempt {
    password: Zeroizing<String>,
}

impl std::fmt::Debug for LoginAttempt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoginAttempt")
            .field("password", &"[redacted secret]")
            .finish()
    }
}
```

Incorrect:

```rust
return Err(anyhow!("proxy authentication failed for {password}"));
```

Correct:

```rust
return Err(anyhow!("proxy authentication failed"));
```

## Review checklist

1. Search the changed path for `password`, `passphrase`, `private_key`, `identity`, `token`, `secret`, `credential`, `authorization`, and prompt answers.
2. Trace each value from input through UI state, runtime configuration, async capture, protocol use, persistence, and drop.
3. Inspect all `Debug`, error, tracing, notification, serialization, and diagnostic paths.
4. Verify that secret stores contain values while settings and connection records contain only safe references or markers.
5. Check every spawned future, thread, and process for secret clones and unbounded lifetime.
6. Audit AI, telemetry, support, and task-capture boundaries separately from ordinary logging.
7. Add focused regression tests proving redaction, non-persistence, and safe legacy defaults.

## Verification

- Assert that `Debug` and user-facing errors do not contain representative secret values.
- Serialize the surrounding metadata and prove the secret is absent.
- Exercise successful, failed, cancelled, and timed-out authentication paths.
- Verify that replacing or dropping the owner clears every owned secret copy.
- Run focused tests near the changed authentication or persistence boundary, followed by the relevant crate checks.
