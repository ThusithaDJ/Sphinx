# Contributing to Sphinx

Thanks for your interest! Bug reports, ideas and pull requests are all welcome.

## Reporting bugs and requesting features

Open a GitHub issue. For bugs, include:

- your OS and Sphinx version
- what you did, what you expected, and what happened instead
- for analysis or upload problems: which AI provider or stock site you were
  using (**never paste API keys or passwords**)

Security problems go through [SECURITY.md](SECURITY.md), not issues.

## Development setup

See [Building from source](README.md#building-from-source) in the README.

## Pull requests

1. Fork the repo and create a branch from `main`.
2. Keep each PR focused on one change.
3. Before opening the PR, make sure these pass:

   ```
   npm run build                          # TypeScript check + frontend build
   cd src-tauri/crates/core && cargo test # engine unit tests
   cd src-tauri && cargo check            # Tauri shell
   ```

4. Add or update tests in `sphinx-core` for engine changes. Tests must not
   touch the network or the real OS credential store.
5. Describe what changed and why, with a screenshot for UI changes.

## Code guidelines

- **Engine logic goes in `src-tauri/crates/core`** (no Tauri dependency, unit
  tested). `src-tauri/src/lib.rs` only exposes it as Tauri commands.
- **Secrets never go into SQLite.** Store them through
  `sphinx_core::secrets`.
- **Database changes are new numbered migrations** in `db.rs`; never edit an
  existing one.
- Match the style of the surrounding code.

By contributing, you agree your contributions are licensed under the
[MIT License](LICENSE).
