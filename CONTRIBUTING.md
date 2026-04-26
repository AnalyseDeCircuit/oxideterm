# Contributing to OxideTerm

Thank you for your interest in OxideTerm! We appreciate every form of contribution — from bug reports and feature ideas to plugins and documentation improvements.

## How to Contribute

### Bug Reports & Feature Requests

The best way to contribute is through [GitHub Issues](https://github.com/AnalyseDeCircuit/OxideTerm/issues). Clear, reproducible bug reports and well-thought-out feature proposals help shape the project's direction.

### Plugins

OxideTerm has a **[plugin system](plugin-development/README.md)** that lets you extend functionality without modifying core code. If you've built something cool, share it with the community!

### Pull Requests

We maintain a high bar for code that enters the core codebase. Before writing any code, **always open an Issue first** to discuss the approach — this avoids wasted effort on changes that may not align with the project's architecture or roadmap.

Please understand that the maintainer team is small, and reviewing, integrating, and long-term maintaining third-party code is a significant commitment. PRs opened without prior discussion may be closed. When in doubt, a plugin is often the better path.

If we do agree on a change, here's what to expect: the PR should be small, focused, well-tested, and link to the relevant Issue.

---

## Building from source

You may still clone and build locally for your own use or for security research (see [SECURITY.md](SECURITY.md)).

### Prerequisites

| Requirement | Notes |
|-------------|--------|
| **Rust** | 1.75+ ([rustup](https://rustup.rs/)) |
| **Node.js** | 18+ |
| **pnpm** | `npm install -g pnpm` |
| **macOS** | Xcode Command Line Tools |
| **Windows** | Visual Studio C++ Build Tools |
| **Linux** | `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev` |

### Commands

```bash
git clone https://github.com/AnalyseDeCircuit/OxideTerm.git
cd OxideTerm
pnpm install

# Full app (frontend + Rust + local PTY)
pnpm tauri dev

# Frontend only (Vite on http://localhost:1420)
pnpm dev

# Production bundle
pnpm tauri build

# Rust only
cd src-tauri && cargo check && cargo fmt

# Lightweight backend (no local PTY)
cd src-tauri && cargo build --no-default-features --release
```

Other useful scripts: `pnpm i18n:check`, `pnpm license:check:backend`, `pnpm project:stats`.

---

## Code Standards

All contributions must meet the following standards. Familiarity with these is expected before opening a PR.

### Process

1. Discuss in an **Issue** before coding — PRs without prior agreement are unlikely to be merged.
2. Keep PRs **small and focused**; link the Issue.
3. Read **[docs/reference/SYSTEM_INVARIANTS.md](docs/reference/SYSTEM_INVARIANTS.md)** before touching session, connection, or reconnect code.

### Internationalization (i18n)

- **No user-visible hardcoded strings** in UI code.
- Add keys to **all 11** locale files under `src/locales/{lang}/`.
- Run `pnpm i18n:check`.

### Frontend (TypeScript / React)

- Prefer `type` over `interface` unless you need `extends`.
- Use function components and hooks; merge classes with `cn()` where the project already does.
- After changing `sessionTreeStore`, sync connection state per invariants (e.g. `refreshConnections()`).

### Backend (Rust)

- Run `cargo fmt` (and `cargo clippy` if you fix warnings in touched code).
- Respect **lock ordering** documented in invariants (no Session lock while holding SessionRegistry, etc.).

### API parity

- New Tauri commands: implement in `src-tauri/src/commands/`, register in `lib.rs`, wrap in `src/lib/api.ts`, types in `src/types/` as needed.
- Keep **frontend and Rust data shapes** in sync.

### Security & hygiene

- Do not commit secrets, real hostnames, or private keys.
- Passwords and API keys belong in the **OS keychain**, not config files.

### Documentation

- Meaningful changes to architecture or behaviour should touch **`docs/reference/`** when appropriate.

---

## Questions

Use [GitHub Discussions Q&A](https://github.com/AnalyseDeCircuit/oxideterm/discussions/categories/q-a) for general questions (not for undisclosed security issues — use [SECURITY.md](SECURITY.md)).

---

## License reminder

The project is under **GPL-3.0**. By contributing (if ever accepted), you agree your contributions are licensed under the same terms unless explicitly stated otherwise.
