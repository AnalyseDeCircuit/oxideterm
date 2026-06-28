# OxideTerm russh Vendor Patches

This directory is a vendored russh fork, not a plain crates.io copy. Before
upgrading it, compare the current tree against the exact upstream russh release
and preserve the OxideTerm-specific compatibility and secret-handling patches
listed below.

## Why russh Is Vendored

OxideTerm hit an OpenSSH compatibility regression with RSA SHA-2 authentication
on strict OpenSSH servers. Newer OpenSSH deployments can reject legacy
`ssh-rsa` SHA-1 signatures and only allow `rsa-sha2-256` or `rsa-sha2-512`.

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

- `src/client/encrypted.rs`
  - Use `certificate_algorithm_name(cert, hash_alg)` for RSA certificate probes
    and signed requests.
  - Pass the certificate `HashAlg` into `client_make_to_sign`.
  - Preserve the custom signer contract: certificate signers return the original
    `to_sign` buffer with an appended length-prefixed signature blob.
- `src/negotiation.rs`
  - Keep NIST P-256/P-384/P-521 ECDH algorithms in the default KEX fallback
    list without re-enabling SHA-1 DH fallbacks.
- `Cargo.toml.orig` and the normalized `Cargo.toml`
  - Keep the RustCrypto prerelease dependency set aligned with the IronRDP
    dependency family used by the RDP helper. Cargo resolves these crates
    workspace-wide, so mismatched `curve25519-dalek`, `ecdsa`,
    `ed25519-dalek`, `elliptic-curve`, `p256`, `p384`, or `p521` pins can
    make SSH and RDP impossible to build together.
- Secret handling patches
  - Redact auth methods and keyboard-interactive responses in `Debug` output.
  - Store queued password and keyboard-interactive responses in `Zeroizing`
    buffers.
  - Zeroize private-key file buffers and DH shared-secret mpints.
  - Do not log passwords in russh examples.

## Verification

The authoritative regression coverage is in the Tauri crate because those tests
were added when this RSA SHA-2 issue was originally found. After changing this
vendor fork, run:

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
