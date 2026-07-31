# OxideTerm russh Vendor Patches

This directory is a vendored russh fork, not a plain crates.io copy. Before
upgrading it, compare the current tree against the exact upstream russh release
and preserve every OxideTerm-specific compatibility, transfer, and
secret-handling patch listed below.

## Exact Upstream Baseline

The current fork is based on russh `0.61.2`, upstream commit
`ff74d7332b717fe6caf56f63aa4decdcdfab8645`. The same commit is recorded in
`.cargo_vcs_info.json`.

Do not use a nearby tag, crates.io archive, or current upstream `main` as the
comparison base. Before upgrading, diff this directory against that exact
commit, then classify each remaining difference using the sections below.

## Why russh Is Vendored

OxideTerm vendors russh for several independently required behaviors:

- correct RSA SHA-2 certificate authentication on strict OpenSSH servers;
- broader modern and opt-in legacy algorithm negotiation;
- sntrup761/X25519 hybrid key exchange on every supported desktop platform;
- owned channel writes and owned stream halves used by the SFTP pipeline;
- explicit zeroization and redaction of authentication and key-exchange data;
- explicit X11 channel admission and zeroizing X11 cookie transport;
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
upstream russh 0.59 and 0.61 still encode the outer public-key algorithm name as
`ssh-rsa-cert-v01@openssh.com`. Strict OpenSSH checks that outer algorithm name
before it inspects the signature blob, so the request is rejected even if the
inner signature uses SHA-256 or SHA-512.

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
    OpenSSH alias, plus malformed-length and shared-secret coverage.

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
- `src/channels/mod.rs`, `src/lib_inner.rs`, `src/channels/benchmark.rs`, and
  `benches/sftp_transport.rs`
  - Keep the public owned-half exports, zero-copy slice regression tests, and
    the optional benchmark comparing borrowed and owned transport paths.

### Dependency Compatibility

- `Cargo.toml.orig` and the normalized `Cargo.toml`
  - Keep the RustCrypto prerelease dependency set aligned with the IronRDP
    dependency family used by the RDP helper. Cargo resolves these crates
    workspace-wide, so mismatched `curve25519-dalek`, `ecdsa`,
    `ed25519-dalek`, `elliptic-curve`, `p256`, `p384`, or `p521` pins can
    make SSH and RDP impossible to build together.

### Secret Handling

- Secret handling patches
  - Redact auth methods and keyboard-interactive responses in `Debug` output.
  - Store queued password and keyboard-interactive responses in `Zeroizing`
    buffers.
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

### X11 Channel Admission

- `src/client/mod.rs` and `src/client/encrypted.rs`
  - Keep `Handler::should_accept_x11_server_channel` as an explicit gate before
    sending channel-open confirmation. Its default must reject, because RFC
    4254 permits server-opened X11 channels only after a successful client
    request.
  - Reject failed admission with `SSH_OPEN_ADMINISTRATIVELY_PROHIBITED`; X11 is
    a known channel type whose use was not authorized for the connection.
  - Do not move this check into `server_channel_open_x11`; that callback runs
    after the protocol has already confirmed the channel.
- `src/channels/mod.rs` and `src/server/encrypted.rs`
  - Keep `X11AuthenticationCookie` zeroizing and redacted while preserving the
    existing public `request_x11` call shape through `AsRef<str>`.

Mechanical cleanups such as replacing `cloned()` with `copied()` or removing
unnecessary clones are not vendor contracts. Re-evaluate those normally during
an upstream rebase instead of preserving them as mandatory patches.

## Verification

After changing this vendor fork, run the local russh and OxideTerm integration
coverage first:

```sh
cargo fmt --check
cargo test -p russh
cargo test -p russh x11_cookie_debug_is_redacted
cargo test -p russh --test test_sntrup_kex
cargo test -p oxideterm-ssh
cargo test -p oxideterm-sftp
cargo check -p oxideterm-gpui-app
git diff --check
```

For transfer-path changes, also run the focused owned-channel tests and optional
benchmark:

```sh
cargo test -p russh channel_tx_write_bytes_preserves_owned_slices
cargo bench -p russh --features _bench --bench sftp_transport
```

The RSA SHA-2 wire regression coverage remains in the Tauri repository because
those tests launch real local OpenSSH servers. Run it when changing certificate
algorithm names, signer packet construction, or RSA agent behavior:

```sh
cd /Users/dominical/Documents/oxideterm-main/src-tauri
cargo test rsa_sha2 -- --test-threads=1
```

The expected coverage is four real local OpenSSH tests:

- agent auth against an `rsa-sha2-256`-only server
- agent auth against an `rsa-sha2-512`-only server
- certificate auth against an `rsa-sha2-256`-only server
- certificate auth against an `rsa-sha2-512`-only server

Mock tests are not enough for this bug because the failures are caused by the
actual SSH wire algorithm name and signature packet shape.

## Upgrade Checklist

When updating russh:

1. Diff against upstream commit `ff74d7332b717fe6caf56f63aa4decdcdfab8645`
   before moving to the proposed new baseline.
2. Check whether upstream now provides equivalent RSA certificate algorithm
   selection, sntrup KEX, owned channel writes, or secret zeroization. Prefer
   the upstream implementation when its behavior and tests cover OxideTerm's
   contract.
3. Reapply only the still-required patches above and update
   `.cargo_vcs_info.json` to the exact new upstream commit.
4. Keep `Cargo.toml.orig` and the normalized `Cargo.toml` synchronized.
5. Verify the safe default algorithm order and the opt-in legacy profile
   separately. Never enable SHA-1 or CBC algorithms in `Preferred::DEFAULT`.
6. Verify both sntrup names on Windows, macOS, Linux x64, and Linux ARM64. A
   target-specific dependency regression must not silently remove an algorithm
   that remains advertised elsewhere.
7. Run the full verification set above before publishing an installer or
   updater manifest that contains the rebased SSH stack.
