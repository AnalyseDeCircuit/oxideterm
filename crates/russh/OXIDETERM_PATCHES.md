# OxideTerm russh Vendor Patches

This directory is a vendored russh fork, not a plain crates.io copy. Before
upgrading it, compare the current tree against the exact upstream russh release
and preserve every OxideTerm-specific compatibility, transfer, and
secret-handling patch listed below.

## Exact Upstream Baseline

The current fork is based on russh `0.63.0`, upstream commit
`dbe2234491fcd50e0bf68438452932f918092fe0`. The same commit is recorded in
`.cargo_vcs_info.json`. The imported crates.io archive has SHA-256
`00cf00190c315093734a8d405225bd8773a219bc86538a9b73bfc51145b33995`.

Do not use a nearby tag, crates.io archive, or current upstream `main` as the
comparison base. Before upgrading, diff this directory against that exact
commit, then classify each remaining difference using the sections below.

## Audited Diff Snapshot

This inventory was verified on 2026-08-21 against the crates.io 0.63.0 package
and its recorded upstream commit. The rebased tree differs in 26 tracked paths:
19 modified paths and 7 added paths. Every path is classified below.

The previous fork was independently audited before the rebase against both
authoritative forms of the 0.61.2 baseline:

- the crates.io `russh-0.61.2.crate` archive, SHA-256
  `bbf893f64684e58da8a68d56a5e84d1cf0440226274c515770fe267707a7d0b0`;
- upstream commit `ff74d7332b717fe6caf56f63aa4decdcdfab8645`, which is also
  recorded in `.cargo_vcs_info.json`.

That pre-rebase tree was repository commit `0467d1fe0`. Compared with the
0.61.2 crates.io archive, it had 30 tracked differences: 23 modified paths and
7 added paths. The historical commit map remains useful for tracing why each
current contract exists.

### Historical Patch Map

| Behavior | Originating OxideTerm commits |
| --- | --- |
| Rebase to 0.61.2 while retaining RSA certificate and security fixes | `82172e5cc` |
| Keyboard-interactive ownership, auth redaction, private-key and DH cleanup | `90d841ea8`, `330098b8c` |
| RustCrypto dependency alignment with IronRDP | `d83174a83` |
| Safe negotiation additions and opt-in legacy compatibility | `00e68c8df`, `94323846b` |
| sntrup761/X25519 implementation and cross-platform dependency correction | `3f587c392`, `a06ebff0a`, `756d8c021`, `e201ab8dc` |
| Owned channel halves and zero-copy SFTP writes | `44b235741` |
| X11 admission, cookie redaction, and outgoing packet cleanup | `7d583e52e` |

Commit `3bbfb4baf` contains only mechanical clone cleanups in the final vendor
diff. It is not a patch contract and should not be replayed during an upgrade.

## Why russh Is Vendored

OxideTerm vendors russh for several independently required behaviors:

- correct RSA SHA-2 certificate authentication on strict OpenSSH servers;
- broader modern and opt-in legacy algorithm negotiation;
- sntrup761/X25519 hybrid key exchange on every supported desktop platform;
- owned channel writes and owned stream halves used by the SFTP pipeline;
- explicit zeroization and redaction of authentication, GSSAPI, and
  key-exchange data;
- application-enforced X11 channel admission and zeroizing X11 cookie
  transport;
- workspace-wide RustCrypto dependency compatibility with IronRDP.

The original compatibility issue was RSA SHA-2 authentication. Newer OpenSSH
deployments can reject legacy `ssh-rsa` SHA-1 signatures and only allow
`rsa-sha2-256` or `rsa-sha2-512`.

The affected paths are:

- direct RSA private-key authentication
- RSA authentication through SSH Agent
- OpenSSH user certificate authentication backed by an RSA key

The certificate path has the most important russh-side protocol issue: passing a
`HashAlg` to `authenticate_certificate_with` controls the signature hash, but
upstream russh 0.59 through 0.63 still encode the outer public-key algorithm
name as `ssh-rsa-cert-v01@openssh.com`. Strict OpenSSH checks that outer
algorithm name before it inspects the signature blob, so the request is rejected
even if the inner signature uses SHA-256 or SHA-512.

For RSA certificates the wire algorithm must be:

- `rsa-sha2-256-cert-v01@openssh.com` when signing with SHA-256
- `rsa-sha2-512-cert-v01@openssh.com` when signing with SHA-512

## Required Local Patches

Keep these patches when updating russh:

### RSA SHA-2 Certificates

- `src/client/encrypted.rs`
  - Use `certificate_algorithm_name(cert, hash_alg)` for RSA certificate probes
    and signed requests.
  - Pass the certificate `HashAlg` into `client_make_to_sign`.
  - Preserve the custom signer contract: certificate signers return the original
    `to_sign` buffer with an appended length-prefixed signature blob.

### Algorithm Negotiation

- `src/negotiation.rs`
  - Keep NIST P-256/P-384/P-521 ECDH algorithms in the default KEX fallback
    list without re-enabling SHA-1 DH fallbacks.
  - Keep both `aes256-gcm@openssh.com` and `aes128-gcm@openssh.com` in the safe
    default cipher list.
  - Keep `Preferred::legacy_compatibility()` separate from the safe default. It
    appends SHA-1 DH, AES-CBC, and SHA-1 MAC choices after modern algorithms so
    legacy mode does not weaken negotiation with modern peers.

### sntrup761/X25519 Hybrid KEX

- `src/kex/hybrid_sntrup761.rs`
  - Implement the OpenSSH-compatible sntrup761 plus X25519 hybrid exchange,
    including message lengths, SHA-512 exchange hashing, and combined-secret
    key derivation.
  - Follow the 0.63.0 Curve25519 hardening: use clamped X25519 multiplication
    and reject low-order peer points that produce an all-zero shared secret.
  - Zeroize the retained X25519 private value and combined-secret scratch.
  - Keep private KEX state redacted from `Debug` output.
- `src/kex/mod.rs`
  - Register both `sntrup761x25519-sha512` and
    `sntrup761x25519-sha512@openssh.com` against the same implementation.
- `src/negotiation.rs`
  - Offer ML-KEM first, both sntrup names next, and Curve25519 afterward. This
    preserves the existing ML-KEM preference while allowing sntrup-only peers.
- `Cargo.toml.orig` and the normalized `Cargo.toml`
  - Use the pure Rust `sntrup` crate rather than the older `sntrup761` crate.
    The latter selected an unsupported `sha2-asm` path on Windows; do not
    reintroduce platform `cfg` gates that make the advertised KEX unavailable
    only on Windows.
- `tests/test_sntrup_kex.rs`
  - Preserve full client/server handshakes for both the standard name and the
    OpenSSH alias, plus malformed-length, low-order point, and shared-secret
    coverage.

### Owned Channel Transport for SFTP

- `src/channels/channel_stream.rs`
  - Keep `ChannelStream::into_split()` and the owned `ChannelStreamReader` and
    `ChannelStreamWriter` halves. The reading half retains channel-close
    ownership.
- `src/channels/io/tx.rs`
  - Keep `ChannelTx::write_bytes(...)` and
    `ChannelStreamWriter::write_bytes(...)` so an owned `Bytes` allocation can
    be sliced across SSH window and maximum-packet boundaries without copying
    each fragment.
  - Preserve window reservation and notification ordering; registering the
    waiter before releasing the window lock prevents a lost adjustment wakeup.
  - Do not overlap the owned send path with a pending borrowed `AsyncWrite`
    send on the same channel.
  - Build on the 0.63.0 channel notification and backpressure behavior rather
    than restoring the 0.61.2 file wholesale.
- `src/channels/mod.rs`, `src/lib_inner.rs`, `src/channels/benchmark.rs`, and
  `benches/sftp_transport.rs`
  - Keep the public owned-half exports, zero-copy slice regression tests, and
    the optional benchmark comparing borrowed and owned transport paths.

### Dependency Compatibility

- `Cargo.toml.orig` and the normalized `Cargo.toml`
  - The attempted stable 0.63.0 dependency set conflicts with IronRDP's exact
    RustCrypto family during workspace resolution. Keep the demonstrated
    compatible versions for `curve25519-dalek`, `ecdsa`, `ed25519-dalek`,
    `p256`, `p384`, `p521`, and `ssh-key` until IronRDP moves to a compatible
    family. Do not add further overrides without a resolver error.
  - Keep workspace `ssh-encoding` on the stable 0.3 release required by russh
    0.63.0 and `russh-cryptovec` 0.62.0.

### Secret Handling

- Secret handling patches
  - Redact auth methods and keyboard-interactive responses in `Debug` output.
  - Store queued password and keyboard-interactive responses in `Zeroizing`
    buffers.
  - Store GSSAPI continuation tokens, final tokens, MIC values, error tokens,
    and MIC input data in `Zeroizing` buffers. `GssapiStep`, `GssapiError`,
    internal replies, and client messages must use redacted `Debug` output.
  - Zeroize private-key file buffers and DH shared-secret mpints.
  - Redact DH private exponents and shared secrets while retaining safe public
    diagnostics.
  - Do not log passwords in russh examples.
  - Store queued X11 authentication cookies in a redacted `Zeroizing<String>`
    wrapper so `ChannelMsg` debug output and normal message drop cannot retain
    or expose the bearer credential.
  - Treat the shared outgoing packet queue as transient plaintext: never trace
    packet payload bytes, wipe each packet after encryption, and wipe pending
    bytes again when the encrypted session is dropped. The X11 wrapper alone is
    insufficient once its cookie has been encoded into the SSH packet buffer.
  - Wipe `PacketWriter::packet_buffer` after compressed packet construction,
    on construction failure, and on drop. Also wipe appended output scratch on
    packet-writing errors before truncating it.

### X11 Channel Admission

- `crates/oxideterm-ssh/src/transport/handler.rs`
  - Use the upstream 0.63.0 `ChannelOpenHandle` in
    `server_channel_open_x11`. Resolve the route and reserve capacity before
    accepting; reject unauthorized channels as `AdministrativelyProhibited`
    and capacity exhaustion as `ResourceShortage`.
  - Do not restore the old vendor-only
    `Handler::should_accept_x11_server_channel` hook. Upstream now delays
    protocol confirmation until the callback explicitly accepts the handle.
- `src/channels/mod.rs` and `src/server/encrypted.rs`
  - Keep `X11AuthenticationCookie` zeroizing and redacted while preserving the
    existing public `request_x11` call shape through `AsRef<str>`.

## Complete 0.63.0 File Inventory

These paths are the complete tracked behavior diff against the official 0.63.0
crate:

| Paths | Vendor behavior |
| --- | --- |
| `Cargo.lock`, `Cargo.toml`, `Cargo.toml.orig` | RustCrypto compatibility versions, pure-Rust `sntrup`, sntrup integration coverage, and the SFTP transport benchmark. |
| `examples/sftp_server.rs` | Stops the example from logging accepted passwords. |
| `src/auth.rs` | Zeroizes passwords and GSSAPI values and redacts auth diagnostics. |
| `src/client/mod.rs`, `src/client/encrypted.rs` | Zeroizes queued keyboard-interactive and GSSAPI data, redacts internal replies/messages, and preserves RSA SHA-2 user-certificate encoding. |
| `src/kex/dh/groups.rs`, `src/kex/dh/mod.rs` | Redacts DH private state and zeroizes encoded shared-secret material. |
| `src/kex/hybrid_sntrup761.rs`, `src/kex/mod.rs`, `tests/test_sntrup_kex.rs` | Implements both OpenSSH sntrup names with clamped X25519, low-order rejection, handshake tests, and malformed-input coverage. |
| `src/negotiation.rs` | Adds sntrup, NIST ECDH fallbacks, AES-128-GCM, and the separate legacy profile; updates the upstream unknown-KEX fixture now that sntrup is implemented. |
| `src/keys/mod.rs` | Zeroizes private-key file contents after parsing. |
| `src/channels/channel_stream.rs`, `src/channels/io/tx.rs`, `src/channels/mod.rs`, `src/lib_inner.rs` | Adds owned stream halves and an owned `Bytes` send path on top of 0.63.0 channel backpressure. These paths also own the redacted X11 cookie wrapper. |
| `src/channels/benchmark.rs`, `benches/sftp_transport.rs` | Measures borrowed writes against the owned SFTP transport path. |
| `src/server/encrypted.rs` | Keeps decoded X11 cookies zeroizing while passing them to the handler. |
| `src/session.rs`, `src/sshbuffer.rs` | Redacts and wipes outgoing plaintext packet queues, compressed packet scratch, failure paths, and drop paths. |

The other tracked additions are not protocol patches:

- `.cargo-ok` and `LICENSE-APACHE` are package/vendor metadata;
- `OXIDETERM_PATCHES.md` is this audit document.

No mechanical 0.61.2 source cleanup remains in the 0.63.0 behavior diff.

## 0.63.0 Rebase Result

| Contract | Result |
| --- | --- |
| RSA SHA-2 user certificates | Reapplied because upstream host-certificate support does not fix external user-certificate algorithm names. |
| Safe and legacy algorithm offers | Reapplied with the new `host_key_certificates` field left at its secure default. |
| sntrup761/X25519 | Reapplied and updated to match 0.63.0 Curve25519 clamping and low-order validation. |
| Owned SFTP stream transport | Reapplied over the 0.63.0 notification/backpressure implementation. |
| Secret handling | Reapplied and extended to GSSAPI and the new compressed `PacketWriter` scratch buffer. |
| X11 admission | Migrated from the removed custom hook to upstream `ChannelOpenHandle`; cookie and packet wiping remain vendor patches. |
| RustCrypto compatibility | Stable upstream resolution was attempted and failed against IronRDP's exact family; only the demonstrated compatibility versions were restored. |
| Host certificates | The new callback type is integrated, but certificates remain unadvertised and explicitly rejected until OxideTerm implements CA, principal, validity, and revocation policy. |
| GSSAPI authentication | RFC 4462 protocol support is present and its data is protected; OxideTerm supplies Apple GSS, Unix GSSAPI, and Windows SSPI adapters in `oxideterm-ssh`. |

Mechanical cleanups such as replacing `cloned()` with `copied()` or removing
unnecessary clones are not vendor contracts. Re-evaluate those normally during
an upstream rebase instead of preserving them as mandatory patches.

## Verification

After changing this vendor fork, run the local russh and OxideTerm integration
coverage first:

```sh
cargo fmt --all --check
cargo test -p russh
cargo test -p russh x11_cookie_debug_is_redacted
cargo test -p russh --test test_sntrup_kex
cargo test -p oxideterm-ssh
cargo test -p oxideterm-sftp
cargo test -p oxideterm-forwarding
cargo check -p oxideterm-gpui-app
git diff --check
```

For transfer-path changes, also run the focused owned-channel tests and optional
benchmark:

```sh
cargo test -p russh channel_tx_write_bytes_preserves_owned_slices
cargo bench -p russh --features _bench --bench sftp_transport
```

The RSA SHA-2 wire regression harness remains in the historical Tauri
repository because those tests launch real local OpenSSH servers. That
repository is pinned to its own vendored russh 0.61.2, so running it unchanged
does not validate this tree. Before using the harness, make a disposable copy
or worktree, point its russh dependency at this vendor directory, and confirm
the resolved path with `cargo tree`. Then run it when changing certificate
algorithm names, signer packet construction, or RSA agent behavior:

```sh
cd /Users/dominical/Documents/oxideterm-main/src-tauri
cargo test rsa_sha2 -- --test-threads=1
```

Never report the historical harness as validation of the current fork unless
its resolved russh dependency points at this directory.

The expected coverage is four real local OpenSSH tests:

- agent auth against an `rsa-sha2-256`-only server
- agent auth against an `rsa-sha2-512`-only server
- certificate auth against an `rsa-sha2-256`-only server
- certificate auth against an `rsa-sha2-512`-only server

Mock tests are not enough for this bug because the failures are caused by the
actual SSH wire algorithm name and signature packet shape.

## Upgrade Checklist

When updating russh:

1. Diff the current tree against upstream commit
   `dbe2234491fcd50e0bf68438452932f918092fe0` and its exact crates.io package.
2. Import the proposed release as a clean baseline and update
   `.cargo_vcs_info.json`; do not replay raw hunks before checking for upstream
   equivalents.
3. Reapply the behavior contracts in the complete file inventory. Prefer a new
   upstream implementation when it preserves the same ownership, performance,
   compatibility, and secret-handling guarantees.
4. Migrate upstream breaking APIs in OxideTerm, russh-sftp examples, and real
   forwarding tests before changing unrelated application code.
5. Resolve the full workspace dependency graph before adding version overrides.
   Keep only conflicts demonstrated by Cargo and regenerate both lockfiles.
6. Keep `Cargo.toml.orig` and the normalized `Cargo.toml` synchronized.
7. Verify the safe default algorithm order and the opt-in legacy profile
   separately. Never enable SHA-1 or CBC algorithms in `Preferred::DEFAULT`.
8. Verify both sntrup names on Windows, macOS, Linux x64, and Linux ARM64. A
   target-specific dependency regression must not silently remove an algorithm
   that remains advertised elsewhere.
9. Audit every new authentication or packet-buffer variant for redaction and
   zeroization before connecting it to application or platform providers.
10. Regenerate this exact diff inventory and run the full verification set
   before publishing an installer or
   updater manifest that contains the rebased SSH stack.
